#!/usr/bin/env python3
"""Cancel obsolete GitHub Actions runs; never cancel a progressing valid run.

Intentionally runs ONLY as trusted default-branch workflow code.
Requires GITHUB_TOKEN with actions:write, pull-requests:read and contents:read.
"""
from __future__ import annotations

import datetime as dt
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request


REPO = os.environ["GITHUB_REPOSITORY"]
TOKEN = os.environ["GH_TOKEN"]
CURRENT_RUN_ID = int(os.environ["GITHUB_RUN_ID"])
API = os.getenv("GITHUB_API_URL", "https://api.github.com").rstrip("/")
NOW = dt.datetime.now(dt.timezone.utc)
MAX_RUNS = 200
BLOCKED_MINUTES = 45


def request(path: str, method: str = "GET"):
    url = path if path.startswith("https://") else API + path
    headers = {
        "Authorization": "Bearer " + TOKEN,
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "safe-actions-queue-pruner",
    }
    req = urllib.request.Request(url, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            payload = resp.read()
            return json.loads(payload) if payload else None
    except urllib.error.HTTPError as err:
        if method == "POST" and err.code == 409:
            return {"conflict": True}
        raise RuntimeError(
            f"GitHub API {method} {path} returned {err.code}: "
            + err.read(300).decode("utf-8", errors="replace")
        ) from err


def list_pages(path: str, result_key: str | None = None):
    out = []
    for page in range(1, 11):
        sep = "&" if "?" in path else "?"
        result = request(f"{path}{sep}per_page=100&page={page}")
        chunk = result[result_key] if result_key else result
        if not isinstance(chunk, list):
            raise RuntimeError(f"Unexpected page shape for {path}")
        out.extend(chunk)
        if len(chunk) < 100:
            return out
    raise RuntimeError(f"Pagination cap reached at {path}; refusing partial cleanup")


def run_age_minutes(run: dict) -> float:
    when = dt.datetime.fromisoformat(run["created_at"].replace("Z", "+00:00"))
    return (NOW - when).total_seconds() / 60


def only_unavailable_self_hosted_jobs(run_id: int) -> bool:
    jobs = list_pages(f"/repos/{REPO}/actions/runs/{run_id}/jobs", "jobs")
    unfinished = [job for job in jobs if job["status"] != "completed"]
    if not unfinished:
        return False
    # Refuse cancellation when even one job could be running on a real runner.
    return all(
        job["status"] == "queued" and "self-hosted" in (job.get("labels") or [])
        for job in unfinished
    )


def choose_reason(run: dict, open_sha: set[str], main_sha: str, default: str) -> str | None:
    event = run.get("event")
    branch = run.get("head_branch")
    sha = run.get("head_sha")
    if event == "pull_request":
        if not sha or sha not in open_sha:
            return "PR closed or head SHA superseded"
        if run_age_minutes(run) >= BLOCKED_MINUTES and only_unavailable_self_hosted_jobs(run["id"]):
            return "current PR run stranded on self-hosted-only queued jobs"
        return None
    if event == "push" and branch == default and sha != main_sha:
        return "superseded main commit"
    if (
        event == "push" and branch == default
        and run_age_minutes(run) >= BLOCKED_MINUTES
        and only_unavailable_self_hosted_jobs(run["id"])
    ):
        return "current main run stranded on self-hosted-only queued jobs"
    # workflow_dispatch, release, and other deliberate test runs are preserved.
    return None


def main() -> int:
    repo_data = request(f"/repos/{REPO}")
    default = repo_data["default_branch"]
    main_ref = request(f"/repos/{REPO}/git/ref/heads/{urllib.parse.quote(default, safe='')}")
    main_sha = main_ref["object"]["sha"]
    prs = list_pages(f"/repos/{REPO}/pulls?state=open")
    open_sha = {pr["head"]["sha"] for pr in prs}
    runs = []
    for status in ("queued", "pending", "waiting"):
        runs += list_pages(f"/repos/{REPO}/actions/runs?status={status}", "workflow_runs")
    # A run can appear during a state transition in two API queries.
    runs = list({run["id"]: run for run in runs}.values())
    if len(runs) > MAX_RUNS:
        raise RuntimeError(f"{len(runs)} queued runs exceeds safety cap {MAX_RUNS}")

    summary = [f"## Stale Actions cleanup — {REPO}", "", f"Scanned {len(runs)} non-running workflows."]
    cancelled = 0
    skipped = 0
    for run in sorted(runs, key=lambda item: item["created_at"]):
        rid = int(run["id"])
        if rid == CURRENT_RUN_ID:
            continue
        reason = choose_reason(run, open_sha, main_sha, default)
        if reason is None:
            skipped += 1
            continue
        # Recheck state before cancellation. Refuse in-progress runs.
        current = request(f"/repos/{REPO}/actions/runs/{rid}")
        if current["status"] not in ("queued", "pending", "waiting"):
            skipped += 1
            continue
        if current["head_sha"] != run["head_sha"]:
            skipped += 1
            continue
        if "stranded" in reason and not only_unavailable_self_hosted_jobs(rid):
            skipped += 1
            continue
        response = request(f"/repos/{REPO}/actions/runs/{rid}/cancel", "POST")
        if response and response.get("conflict"):
            skipped += 1
            continue
        cancelled += 1
        print(f"Cancellation accepted: run {rid}: {reason}")
        summary.append(f"- Cancel requested for run {rid}: {reason}")

    summary += [
        "",
        f"Cancellation requests: {cancelled}. Preserved: {skipped}.",
        "Cancellation is a request; GitHub may need time to change the run status.",
        "No in-progress runs were cancelled. No branch protection or required checks were modified.",
    ]
    print("\n".join(summary))
    if os.getenv("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as dest:
            dest.write("\n".join(summary) + "\n")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, KeyError, RuntimeError) as exc:
        print(f"::error::Queue cleanup failed closed: {exc}", file=sys.stderr)
        sys.exit(1)

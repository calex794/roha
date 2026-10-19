#!/usr/bin/env python3
"""Audit one frozen STAYA/Iroh SHA batch by full fork-vs-upstream branch history.

The labels are evidence categories, not decisions to merge into Roha/noq.
"""
import argparse
import collections
import concurrent.futures
import csv
import datetime
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path("docs/staya/iroh-fork-census/2026-10-18")
SIZES = [104] * 11 + [103] * 6
TOKEN = os.environ.get("GH_TOKEN", "")
HEADERS = {"Accept": "application/vnd.github+json",
           "X-GitHub-Api-Version": "2022-11-28",
           "User-Agent": "staya-iroh-part-history-audit"}
if TOKEN:
    HEADERS["Authorization"] = "Bearer " + TOKEN

def api(path):
    req = urllib.request.Request("https://api.github.com" + path, headers=HEADERS)
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=50) as response:
                return json.load(response), ""
        except urllib.error.HTTPError as exc:
            body = exc.read(1000).decode("utf-8", "replace")
            if exc.code == 404:
                return None, "HTTP 404: " + body[:300]
            if exc.code not in (403, 429, 500, 502, 503, 504) or attempt == 4:
                return None, "HTTP %s: %s" % (exc.code, body[:300])
            reset = int(exc.headers.get("X-RateLimit-Reset", "0") or "0")
            if exc.headers.get("X-RateLimit-Remaining") == "0":
                delay = max(2, min(85, reset - int(time.time()) + 2))
            else:
                delay = min(40, 2 ** (attempt + 2))
            time.sleep(delay)
        except (TimeoutError, urllib.error.URLError, ConnectionError) as exc:
            if attempt == 4:
                return None, "network error: " + str(exc)[:300]
            time.sleep(min(15, 2 ** (attempt + 1)))
    return None, "retries exhausted"

def is_code(path):
    return path.endswith((".rs", ".c", ".cpp", ".h", ".go", ".js", ".ts"))

def review(row):
    repo, sha = row["representative_repo"], row["sha"]
    out = {
        "ordinal": row["ordinal"], "sha": sha, "repo": repo,
        "branch": row["representative_branch"], "default_class": row["default_class"],
        "source_url": "https://github.com/%s/commit/%s" % (repo, sha),
        "ancestry": "unknown", "ahead": None, "behind": None,
        "merge_base": "", "total_unique_commits": None, "listed_commits": 0,
        "commit_list_complete": False, "changed_file_count": None,
        "code_file_count": None, "changed_code_paths": [], "commit_titles": [],
        "head_commit_title": "", "review_label": "requires_review",
        "evidence_error": ""
    }
    path = "/repos/%s/compare/%s" % (
        repo, urllib.parse.quote("n0-computer:main..." + sha, safe=":."))
    cmp, error = api(path + "?per_page=100&page=1")
    if cmp is None:
        if "No common ancestor" in error:
            out["ancestry"] = "no_common_ancestor"
            out["review_label"] = "separate_historical_lineage"
        else:
            out["ancestry"] = "compare_failed"
            out["evidence_error"] = error
        head, head_error = api("/repos/%s/commits/%s" % (repo, sha))
        if head:
            out["head_commit_title"] = head.get("commit", {}).get("message", "").splitlines()[0]
            files = head.get("files", [])
            code = [f.get("filename", "") for f in files if is_code(f.get("filename", ""))]
            out["changed_file_count"] = len(files)
            out["code_file_count"] = len(code)
            out["changed_code_paths"] = code[:35]
        elif not out["evidence_error"]:
            out["evidence_error"] = head_error
        return out
    out["ancestry"] = cmp.get("status", "unknown")
    out["ahead"] = cmp.get("ahead_by")
    out["behind"] = cmp.get("behind_by")
    out["merge_base"] = (cmp.get("merge_base_commit") or {}).get("sha", "")
    total = cmp.get("total_commits", out["ahead"]) or 0
    out["total_unique_commits"] = total
    files = cmp.get("files", [])
    code = [f.get("filename", "") for f in files if is_code(f.get("filename", ""))]
    out["changed_file_count"] = len(files)
    out["code_file_count"] = len(code)
    out["changed_code_paths"] = code[:35]
    commits = cmp.get("commits", [])
    max_records = 500
    if total and len(commits) < min(total, max_records):
        end_page = 1 + (min(total, max_records) + 99) // 100
        for page in range(2, end_page + 1):
            next_page, page_error = api(path + "?per_page=100&page=%s" % page)
            if next_page is None:
                out["evidence_error"] = "page %s: %s" % (page, page_error)
                break
            batch = next_page.get("commits", [])
            if not batch:
                break
            commits.extend(batch)
            if len(commits) >= min(total, max_records):
                break
    out["listed_commits"] = len(commits)
    out["commit_list_complete"] = (total <= len(commits) and not out["evidence_error"])
    out["commit_titles"] = [
        c.get("commit", {}).get("message", "").splitlines()[0]
        for c in commits[:max_records]
    ]
    out["head_commit_title"] = out["commit_titles"][-1] if commits else ""
    if out["ahead"] == 0:
        out["review_label"] = "no_unique_commits"
    elif not code:
        out["review_label"] = "non_source_diff"
    elif any(f.startswith(("iroh-net/", "iroh-bitswap/", "iroh-p2p/", "iroh-sync/")) for f in code):
        out["review_label"] = "legacy_architecture_manual_mapping"
    elif not out["commit_list_complete"] or len(files) >= 300:
        out["review_label"] = "large_or_truncated_requires_review"
    else:
        out["review_label"] = "source_changes_require_roha_comparison"
    return out

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--part", type=int, required=True, choices=range(1, 18))
    parser.add_argument("--workers", type=int, default=3)
    parser.add_argument("--dry-run", action="store_true")
    opts = parser.parse_args()
    with (ROOT / "history_filtered_shas.csv").open(newline="", encoding="utf-8") as stream:
        all_rows = [r for r in csv.DictReader(stream)
                    if r["status"] == "unreachable_from_current_upstream_refs"]
    assert len(all_rows) == 1762, "Frozen input count changed"
    start = sum(SIZES[:opts.part - 1])
    chunk = all_rows[start:start + SIZES[opts.part - 1]]
    assert len(chunk) == SIZES[opts.part - 1]
    for number, row in enumerate(chunk, start + 1):
        row["ordinal"] = number
    print("PART", opts.part, "RANGE", start + 1, start + len(chunk), flush=True)
    if opts.dry_run:
        return
    if not TOKEN:
        raise RuntimeError("GH_TOKEN required")
    with concurrent.futures.ThreadPoolExecutor(max_workers=opts.workers) as pool:
        results = list(pool.map(review, chunk))
    assert len(results) == len(chunk)
    dest = ROOT / "part_review"
    dest.mkdir(exist_ok=True)
    prefix = "part_%02d" % opts.part
    with (dest / (prefix + "_history.jsonl")).open("w", encoding="utf-8") as stream:
        for item in results:
            stream.write(json.dumps(item, ensure_ascii=False) + "\n")
    cols = ["ordinal", "sha", "repo", "branch", "ancestry", "ahead", "behind",
            "merge_base", "total_unique_commits", "listed_commits",
            "commit_list_complete", "changed_file_count", "code_file_count",
            "head_commit_title", "review_label", "evidence_error", "source_url"]
    with (dest / (prefix + "_history.csv")).open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=cols)
        writer.writeheader()
        for item in results:
            writer.writerow({key: item[key] for key in cols})
    counts = collections.Counter(item["review_label"] for item in results)
    partial = [x for x in results if not x["commit_list_complete"]]
    lines = [
        "# STAYA, Iroh history part %02d" % opts.part, "",
        "Source: frozen history_filtered_shas.csv, 2026-10-18",
        "Reviewed UTC: " + datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "Range: %s-%s / 1762" % (start + 1, start + len(chunk)), "",
        "## Evidence labels (NOT final implementation decisions)", "",
        "| Label | Heads |", "|---|---:|"
    ]
    lines.extend("| %s | %s |" % pair for pair in sorted(counts.items()))
    lines.extend(["", "## Incomplete histories", ""])
    lines.extend(
        "- [%s](%s) %s:%s, %s of %s unique commits" % (
            x["sha"][:12], x["source_url"], x["repo"], x["branch"],
            x["listed_commits"], x["total_unique_commits"])
        for x in partial)
    lines.extend(["", "## Boundaries",
        "- Every SHA is a branch head, not necessarily a separate fix.",
        "- A branch comparison to upstream is not a diff against calex794/roha.",
        "- No entry has been marked merged, tested, or released by this script.",
        "- Without a common ancestor or with truncated files, manual investigation remains."])
    (dest / (prefix + "_SUMMARY.md")).write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("DONE", json.dumps(dict(counts)), "PARTIAL", len(partial), flush=True)

if __name__ == "__main__":
    main()

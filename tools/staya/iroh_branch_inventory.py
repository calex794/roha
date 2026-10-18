#!/usr/bin/env python3
"""Stage 2: list non-default branches for every available Iroh fork.

Never call an empty default branch evidence that the *whole* fork is empty.
"""
import concurrent.futures
import csv
import datetime as dt
import json
import os
import pathlib
import random
import time
import urllib.error
import urllib.parse
import urllib.request

HERE = pathlib.Path("docs/staya/iroh-fork-census/2026-10-18")
DATA = HERE / "all_forks.json"
TOKEN = os.environ["GH_TOKEN"]
HEADERS = {
    "Accept": "application/vnd.github+json",
    "User-Agent": "akula-staya-branch-survey",
    "X-GitHub-Api-Version": "2022-11-28",
    "Authorization": "Bearer " + TOKEN,
}
def api(path):
    for attempt in range(5):
        try:
            with urllib.request.urlopen(
                urllib.request.Request("https://api.github.com" + path,
                                       headers=HEADERS), timeout=35
            ) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            if exc.code in (403, 429, 502, 503, 504) and attempt < 4:
                if exc.headers.get("X-RateLimit-Remaining") == "0":
                    delay = max(2, min(95, int(exc.headers.get(
                        "X-RateLimit-Reset", 0)) - int(time.time()) + 2))
                else:
                    delay = 3 * 2 ** attempt + random.random()
                time.sleep(delay)
                continue
            return {"error": "%s: %s" % (exc.code, exc.read(180).decode(
                "utf8", "replace"))}
        except (TimeoutError, urllib.error.URLError) as exc:
            if attempt == 4:
                return {"error": str(exc)}
            time.sleep(attempt + 2)
    return {"error": "retries exhausted"}

def inspect(row):
    repo = row["full_name"]
    default = row["default_branch"]
    info = {"repo": repo, "class": row["classification"],
            "default": default, "branches": [],
            "other_count": 0, "truncated": False, "error": ""}
    for page in range(1, 11):
        batch = api("/repos/%s/branches?per_page=100&page=%s" % (repo, page))
        if batch is None:
            info["error"] = "repository or branches unavailable"
            break
        if not isinstance(batch, list):
            info["error"] = str(batch)[:260]
            break
        info["branches"].extend(b["name"] for b in batch if b.get("name"))
        if len(batch) < 100:
            break
        if page == 10:
            info["truncated"] = True
    other = [n for n in info["branches"] if n != default]
    info["other_count"] = len(other)
    info["non_default_examples"] = other[:35]
    # Branch-name hints only, not proof that code differs from upstream.
    info["promising_names"] = [
        n for n in other if any(
            marker in n.lower() for marker in (
                "feat", "feature", "fix", "perf", "relay", "quic", "p2p",
                "nat", "multi", "route", "tcp", "dns", "auth", "patch",
                "direct", "discovery", "socket", "ipv6", "mesh", "test"
            )
        )
    ][:50]
    return info

def main():
    src = json.loads(DATA.read_text(encoding="utf-8"))
    rows = src["rows"]
    print("STAGE2_FORKS", len(rows), flush=True)
    result = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for z in pool.map(inspect, rows):
            result.append(z)
            if len(result) % 100 == 0:
                print("STAGE2_DONE", len(result), flush=True)
    result.sort(key=lambda z: (-len(z["promising_names"]),
                                -z["other_count"], z["repo"]))
    (HERE / "branch_inventory.json").write_text(
        json.dumps({"date": dt.datetime.now(dt.timezone.utc).isoformat(),
                    "results": result}, ensure_ascii=False, indent=2),
        encoding="utf-8")
    fields = ["repo", "class", "default", "other_count", "truncated",
              "error", "promising", "examples"]
    with (HERE / "branch_inventory.csv").open(
        "w", encoding="utf8", newline=""
    ) as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        for z in result:
            writer.writerow({
                "repo": z["repo"], "class": z["class"],
                "default": z["default"], "other_count": z["other_count"],
                "truncated": z["truncated"], "error": z["error"],
                "promising": ";".join(z["promising_names"]),
                "examples": ";".join(z["non_default_examples"])
            })
    counts = {
        "examined": len(result),
        "error": sum(bool(z["error"]) for z in result),
        "has_other_branches": sum(z["other_count"] > 0 for z in result),
        "named_feature_branch": sum(bool(z["promising_names"]) for z in result),
        "branch_list_truncated": sum(z["truncated"] for z in result),
        "total_other_branches_observed": sum(z["other_count"] for z in result),
    }
    (HERE / "BRANCH-SUMMARY.md").write_text(
        "# СТАЯ — второй проход, ветки Iroh\n\n"
        + "\n".join("- %s: **%s**" % x for x in counts.items())
        + "\n\nСписок названий веток сам по себе не доказывает, что "
        "код отличается от upstream. Следующий этап — сравнительный "
        "анализ содержимого неосновных веток.\n", encoding="utf8")
    print("STAGE2_RESULT", json.dumps(counts), flush=True)

if __name__ == "__main__":
    main()

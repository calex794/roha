#!/usr/bin/env python3
"""Third pass: deduplicate branches of the Iroh fork family by HEAD SHA.

A SHA shared with upstream is a proven unchanged ref; a distinct SHA is only
a candidate and must NOT be treated as an independently developed feature.
"""
import collections
import concurrent.futures
import csv
import datetime
import json
import os
from pathlib import Path
import random
import time
import urllib.error
import urllib.request

DEST = Path("docs/staya/iroh-fork-census/2026-10-18")
TOKEN = os.environ["GH_TOKEN"]
HEADERS = {
    "Accept": "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "staya-iroh-sha-dedup",
    "Authorization": "Bearer " + TOKEN,
}
LABEL_MARKERS = (
    "feature", "feat/", "fix/", "relay", "quic", "p2p", "multi",
    "addr", "route", "direct", "discovery", "socket", "auth",
    "dns", "udp", "net", "path", "perf", "nat", "transport",
    "android", "linux", "ipv6", "mesh", "peer", "tls",
)


def api(path):
    req = urllib.request.Request("https://api.github.com" + path,
                                 headers=HEADERS)
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=40) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            if exc.code in (403, 429, 502, 503, 504) and attempt < 4:
                remaining = exc.headers.get("X-RateLimit-Remaining")
                if remaining == "0":
                    reset = int(exc.headers.get("X-RateLimit-Reset", "0"))
                    delay = max(2, min(90, reset - int(time.time()) + 3))
                else:
                    delay = min(45, 2 ** (attempt + 2)) + random.random()
                time.sleep(delay)
                continue
            return {"_api_error": str(exc)}
        except (TimeoutError, urllib.error.URLError) as exc:
            if attempt == 4:
                return {"_api_error": str(exc)}
            time.sleep(2 * (attempt + 1))
    return {"_api_error": "retries exhausted"}


def refs_for(repo):
    refs = []
    truncated = False
    error = ""
    for page in range(1, 11):
        data = api("/repos/%s/branches?per_page=100&page=%s" %
                   (repo, page))
        if data is None:
            error = "404 (repository or branches unavailable)"
            break
        if not isinstance(data, list):
            error = str(data)[:200]
            break
        for item in data:
            name = item.get("name")
            sha = item.get("commit", {}).get("sha")
            if name and sha:
                refs.append((name, sha))
        if len(data) < 100:
            break
        if page == 10:
            truncated = True
    return repo, refs, error, truncated


def main():
    with (DEST / "all_forks.csv").open(
        newline="", encoding="utf-8"
    ) as stream:
        forks = list(csv.DictReader(stream))
    assert len(forks) >= 900, "stage-one census incomplete"
    print("SHA_START", len(forks), flush=True)
    root, refs, error, truncated = refs_for("n0-computer/iroh")
    if error or truncated or not refs:
        raise RuntimeError("Upstream refs incomplete: " + error)
    upstream_by_name = dict(refs)
    upstream_sha = set(upstream_by_name.values())
    print("UPSTREAM_REFS", len(refs), "UNIQUE_SHAS", len(upstream_sha),
          flush=True)
    all_refs = []
    summaries = []
    seen = collections.Counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for i, result in enumerate(pool.map(
            refs_for, [item["full_name"] for item in forks]
        ), 1):
            repo, repo_refs, err, trunc = result
            fork = next(x for x in forks if x["full_name"] == repo)
            extras = [(n, s) for n, s in repo_refs
                      if n != fork["default_branch"]]
            matched_sha = sum(s in upstream_sha for _, s in extras)
            matched_name = sum(upstream_by_name.get(n) == s
                               for n, s in extras)
            summary = {
                "repo": repo,
                "default_class": fork["classification"],
                "default_branch": fork["default_branch"],
                "fork_pushed_at": fork["pushed_at"],
                "other_branches": len(extras),
                "upstream_sha_match": matched_sha,
                "upstream_name_and_sha_match": matched_name,
                "unmatched_sha_count": len(extras) - matched_sha,
                "error": err,
                "truncated": trunc,
            }
            summaries.append(summary)
            for name, sha in extras:
                seen[sha] += 1
                all_refs.append({
                    "repo": repo, "branch": name, "sha": sha,
                    "upstream_sha_equal": sha in upstream_sha,
                    "upstream_name_sha_equal":
                        upstream_by_name.get(name) == sha,
                    "default_class": fork["classification"],
                    "fork_pushed_at": fork["pushed_at"],
                })
            if i % 100 == 0 or i == len(forks):
                print("SHA_SCANNED", i, "/", len(forks),
                      "refs", len(all_refs), flush=True)
    for row in all_refs:
        row["same_sha_across_forks"] = seen[row["sha"]]
        row["interesting_label"] = any(
            marker in row["branch"].lower() for marker in LABEL_MARKERS
        )

    # Full refs are kept to allow later regression audits.
    head_file = DEST / "all_branch_heads.csv"
    with head_file.open("w", newline="", encoding="utf-8") as stream:
        fields = [
            "repo", "branch", "sha", "upstream_sha_equal",
            "upstream_name_sha_equal", "same_sha_across_forks",
            "interesting_label", "default_class", "fork_pushed_at",
        ]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(all_refs)
    summary_file = DEST / "fork_branch_sha_summary.csv"
    with summary_file.open("w", newline="", encoding="utf-8") as stream:
        fields = [
            "repo", "default_class", "default_branch", "fork_pushed_at",
            "other_branches", "upstream_sha_match",
            "upstream_name_and_sha_match", "unmatched_sha_count",
            "error", "truncated",
        ]
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(summaries)

    # Exclude identical upstream SHAs. Prefer rare, recent, descriptively
    # named unique SHA representatives; this is triage, NOT a feature verdict.
    candidates = {}
    for row in all_refs:
        if row["upstream_sha_equal"]:
            continue
        sha = row["sha"]
        score = (
            (300 if row["interesting_label"] else 0)
            + (50 if row["default_class"] == "main_outdated" else 0)
            + max(0, 120 - seen[sha])
        )
        if sha not in candidates or score > candidates[sha][0]:
            candidates[sha] = (score, row)
    ranked = [
        row for _, row in sorted(
            candidates.values(), key=lambda x:
            (-x[0], x[1]["repo"], x[1]["branch"])
        )
    ]
    with (DEST / "branch_sha_priority.csv").open(
        "w", newline="", encoding="utf-8"
    ) as stream:
        fields = [
            "repo", "branch", "sha", "same_sha_across_forks",
            "interesting_label", "default_class", "fork_pushed_at",
            "upstream_sha_equal",
        ]
        writer = csv.DictWriter(stream, fieldnames=fields,
                                extrasaction="ignore")
        writer.writeheader()
        writer.writerows(ranked[:600])

    counter = {
        "fork_rows": len(forks),
        "upstream_branches": len(refs),
        "upstream_unique_head_shas": len(upstream_sha),
        "non_default_ref_occurrences": len(all_refs),
        "unique_head_shas": len(seen),
        "upstream_head_sha_duplicates": sum(
            row["upstream_sha_equal"] for row in all_refs
        ),
        "upstream_name_sha_duplicates": sum(
            row["upstream_name_sha_equal"] for row in all_refs
        ),
        "non_upstream_ref_occurrences": sum(
            not row["upstream_sha_equal"] for row in all_refs
        ),
        "unique_non_upstream_head_shas": len(candidates),
        "forks_with_other_branches": sum(
            row["other_branches"] > 0 for row in summaries
        ),
        "branch_list_errors": sum(bool(row["error"]) for row in summaries),
        "truncated": sum(row["truncated"] for row in summaries),
        "forks_with_non_upstream_shas": sum(
            row["unmatched_sha_count"] > 0 for row in summaries
        ),
    }
    md = [
        "# СТАЯ — устранение повторов веток Iroh по SHA",
        "",
        "Время: %s UTC" %
        datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "",
        "| Показатель | Количество |",
        "|---|---:|",
    ]
    for name, value in counter.items():
        md.append("| %s | %d |" % (name, value))
    md += [
        "",
        "## Что эти цифры действительно означают",
        "",
        "- Совпадение SHA c текущей веткой upstream точно означает ту же"
        " версию commit, а не новую независимую разработку.",
        "- SHA, не совпадающий с текущей вершиной upstream, НЕ означает"
        " наличие новых функций. Возможно старая, удалённая upstream"
        " ветка или commit той же истории.",
        "- Если SHA встречается во многих форках, достаточно одного"
        " подробного анализа соответствующего commit; но не выбрасывать"
        " саму ветку без проверки, откуда она произошла.",
        "- Таблица branch_sha_priority.csv — очередь проверки, не"
        " автоматически подтверждённые улучшения.",
        "- Полный перечень all_branch_heads.csv остаётся в отдельной"
        " ветке и позволяет восстановить источники.",
        "",
        "## Первая двадцатка кандидатов по формальному отбору",
        "",
        "| Форк | Ветка | Число повторений SHA |",
        "|---|---|---:|",
    ]
    for row in ranked[:20]:
        md.append("| [%s](https://github.com/%s) | %s | %d |" %
                  (row["repo"], row["repo"], row["branch"].replace("|", "/"),
                   row["same_sha_across_forks"]))
    (DEST / "SHA-SUMMARY.md").write_text(
        "\n".join(md) + "\n", encoding="utf-8"
    )
    print("SHA_RESULT", json.dumps(counter), flush=True)
    print("SHA_TOP", json.dumps([
        {"repo": r["repo"], "branch": r["branch"],
         "sha": r["sha"], "freq": r["same_sha_across_forks"]}
        for r in ranked[:25]
    ], ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Remove old upstream commits from fork-branch SHA candidates.

This checks commit reachability from the entire current upstream main
history and the other current upstream branches. Unreachable does NOT
prove originality: deleted branches, rewritten histories and merges
still need later independent review.
"""
import collections
import csv
import datetime
import json
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path("docs/staya/iroh-fork-census/2026-10-18")
UPSTREAM = "https://github.com/n0-computer/iroh.git"
MARKERS = ("feat", "feature", "fix", "relay", "quic", "path",
           "route", "discovery", "direct", "udp", "tcp", "multi",
           "mesh", "nat", "socket", "dns", "tls", "transport",
           "auth", "perf", "android", "ipv6", "identity", "peer")


def run(*args):
    p = subprocess.run(list(args), capture_output=True, text=True)
    if p.returncode:
        raise RuntimeError("%s failed: %s" %
                           (" ".join(args), p.stderr[-3000:]))
    return p.stdout


def history(graph, *refs):
    raw = run("git", "--git-dir=" + graph, "rev-list", *refs)
    return {s for s in raw.splitlines()
            if re.fullmatch(r"[0-9a-f]{40}", s)}


def main():
    with (ROOT / "all_branch_heads.csv").open(
        encoding="utf-8", newline=""
    ) as f:
        heads = list(csv.DictReader(f))
    candidates = collections.defaultdict(list)
    for r in heads:
        sha = r["sha"]
        if r["upstream_sha_equal"].lower() != "true" and re.fullmatch(
            r"[0-9a-f]{40}", sha
        ):
            candidates[sha].append(r)
    print("HISTORY_BEGIN", len(heads),
          "refs and", len(candidates), "candidate SHAs", flush=True)
    if not 1900 <= len(candidates) <= 2100:
        raise RuntimeError("Unanticipated input count: " +
                           str(len(candidates)))

    with tempfile.TemporaryDirectory(prefix="staya-graph-") as wd:
        path = str(Path(wd) / "upstream.git")
        print("HISTORY_CLONE", UPSTREAM, flush=True)
        run("git", "-c", "protocol.version=2", "clone",
            "--bare", "--filter=blob:none", "--no-tags",
            "--quiet", UPSTREAM, path)
        main_shas = history(path, "refs/heads/main")
        all_shas = history(path, "--all")
        print("HISTORY_GRAPH", len(main_shas), "main",
              len(all_shas), "all", flush=True)
        if len(main_shas) < 1000 or len(all_shas) < len(main_shas):
            raise RuntimeError("Upstream history unexpectedly incomplete")

    output = []
    stats = collections.Counter()
    for sha, refs in candidates.items():
        if sha in main_shas:
            cls = "already_in_upstream_main_history"
        elif sha in all_shas:
            cls = "already_in_other_upstream_branch_history"
        else:
            cls = "unreachable_from_current_upstream_refs"
        stats[cls] += 1
        def rating(r):
            label = r["branch"].lower()
            return (250 if any(x in label for x in MARKERS) else 0) + (
                70 if r["default_class"] == "own_network_code" else 0
            ) + (25 if r["default_class"] == "main_outdated" else 0)
        best = max(refs, key=rating)
        output.append({
            "sha": sha, "status": cls,
            "references": len(refs),
            "repositories": len(set(r["repo"] for r in refs)),
            "representative_repo": best["repo"],
            "representative_branch": best["branch"],
            "default_class": best["default_class"],
            "pushed_at": best["fork_pushed_at"],
            "score": rating(best),
            "more_references": ";".join(
                r["repo"] + ":" + r["branch"] for r in refs[:8]),
        })
    output.sort(key=lambda r: (
        r["status"] != "unreachable_from_current_upstream_refs",
        -r["score"], -r["repositories"], r["representative_repo"],
        r["representative_branch"],
    ))
    fields = list(output[0])
    for name, rows in [
        ("history_filtered_shas.csv", output),
        ("potential_independent_commits.csv", [
            r for r in output
            if r["status"] == "unreachable_from_current_upstream_refs"
        ]),
    ]:
        with (ROOT / name).open("w", encoding="utf-8",
                                newline="") as f:
            wr = csv.DictWriter(f, fieldnames=fields)
            wr.writeheader()
            wr.writerows(rows)

    outstanding = [r for r in output if
                   r["status"] == "unreachable_from_current_upstream_refs"]
    md = [
        "# СТАЯ — исключение исторических коммитов исходного Iroh",
        "",
        "Дата: " + datetime.datetime.now(
            datetime.timezone.utc).isoformat(),
        "",
        "| Критерий | Количество |",
        "|---|---:|",
        "| Кандидатов уникальных SHA до фильтра | %s |" %
        len(candidates),
        "| Старые коммиты основной истории upstream main | %s |" %
        stats["already_in_upstream_main_history"],
        "| Коммиты в историях иных текущих upstream branches | %s |" %
        stats["already_in_other_upstream_branch_history"],
        "| Осталось SHA, недостижимых из актуальных upstream refs | %s |" %
        len(outstanding),
        "| Коммитов в upstream main history | %s |" % len(main_shas),
        "| Коммитов в upstream all refs history | %s |" % len(all_shas),
        "",
        "**Ограничение:** недостижимость НЕ означает полезную или"
        " самостоятельную разработку. Это могут быть исторические"
        " удалённые ветки, коммиты только с документацией и прочее.",
        "Выполнена полная загрузка графа upstream Git с фильтром blob:none,"
        " без shallow/depth. Подтверждённые совпадения исключены.",
        "",
        "## Первые 40 оставшихся SHA по формальной сортировке",
        "",
        "| Форк | Ветка | SHA | Повторов |",
        "|---|---|---|---:|",
    ]
    for r in outstanding[:40]:
        md.append("| [%s](https://github.com/%s) | %s | %s | %s |" %
                  (r["representative_repo"],
                   r["representative_repo"],
                   r["representative_branch"].replace("|", "/"),
                   r["sha"][:12], r["references"]))
    md += [
        "",
        "Следующий этап — найти общий предок форка с upstream,"
        " сравнить собственные коммиты и patch-id; окончательно"
        " классифицировать изменения сетевого кода, а не только SHA.",
        "",
        "Машинные файлы: history_filtered_shas.csv и"
        " potential_independent_commits.csv.",
    ]
    (ROOT / "HISTORY-SUMMARY.md").write_text(
        "\n".join(md) + "\n", encoding="utf-8")
    print("HISTORY_RESULT", json.dumps({
        "input_candidate_shas": len(candidates),
        "historical_main": stats["already_in_upstream_main_history"],
        "historical_other_branches":
            stats["already_in_other_upstream_branch_history"],
        "remaining_candidate_shas": len(outstanding),
        "upstream_main_commits": len(main_shas),
        "upstream_all_commits": len(all_shas)
    }, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()

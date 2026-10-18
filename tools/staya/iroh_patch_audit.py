#!/usr/bin/env python3
"""Audit all post-history-filter Iroh fork branch heads by Git patch-id.

Every SHA is a BRANCH HEAD, not necessarily an individual feature commit.
Full net-diff patch IDs and individual commit patch IDs are separate evidence.
Counts from this program describe syntactic equivalence, NOT semantic quality.

Run inside the isolated Roha census lab branch via GitHub Actions.
"""
from __future__ import annotations
import argparse
from collections import Counter, defaultdict
import csv
from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path("docs/staya/iroh-fork-census/2026-10-18")
INPUT = ROOT / "potential_independent_commits.csv"
OUT = ROOT / "patch-audit"
UPSTREAM = "https://github.com/n0-computer/iroh.git"
SHARD_COUNT = 8
MAX_COMMIT_DETAILS = 80
MAX_PATHS_FOR_PATCH = 300
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
REPO_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")

BRANCH_FIELDS = [
    "sha", "repo", "branch", "reference_count", "default_class", "status",
    "ahead_commits", "merge_base", "net_changed_files", "net_network_files",
    "net_identity_files", "net_test_files", "net_other_code_files",
    "net_doc_config_files", "net_patch_id", "network_patch_id",
    "independent_commit_count", "commit_detail_complete", "sample_changed_files",
    "detail", "github_url",
]
COMMIT_FIELDS = ["commit_sha", "patch_id", "network_patch_id", "code_class",
                 "changed_files", "subject", "branch_head", "repo", "github_url"]


def run(*cmd, cwd=None, check=True, timeout=240):
    p = subprocess.run(cmd, cwd=cwd, text=True, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=timeout,
                       env={**os.environ, "GIT_TERMINAL_PROMPT": "0"})
    if check and p.returncode:
        raise RuntimeError("%s: %s" % (" ".join(cmd[:4]), p.stderr[-650:].strip()))
    return p


def git(directory, *args, check=True, timeout=240):
    return run("git", "--git-dir=" + str(directory), *args,
               check=check, timeout=timeout)


def read_candidates():
    with INPUT.open(newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    if len(rows) != 1762 or len({r["sha"] for r in rows}) != len(rows):
        raise RuntimeError("Candidate snapshot changed; expected exactly 1762 distinct SHAs")
    for r in rows:
        if not SHA_RE.fullmatch(r["sha"]) or not REPO_RE.fullmatch(r["representative_repo"]):
            raise RuntimeError("Invalid repo or SHA in input")
        if not r["representative_branch"] or "\x00" in r["representative_branch"]:
            raise RuntimeError("Invalid branch")
    return rows


def shard_for(sha, shards):
    return int(sha[:8], 16) % shards


def fetch_many(repo_dir, remote, refs):
    """Fetch refs into one shared object store. Split failing batches to isolate bad refs."""
    if not refs:
        return {}
    names = ["+refs/heads/%s:refs/staya/%s" % (x["representative_branch"], x["sha"])
             for x in refs]
    cmd = ["git", "--git-dir=" + str(repo_dir), "-c", "gc.auto=0",
           "fetch", "--quiet", "--no-tags", "--no-write-fetch-head", remote] + names
    try:
        out = run(*cmd, timeout=360)
        return {r["sha"]: "" for r in refs}
    except Exception as exc:
        if len(refs) == 1:
            sha = refs[0]["sha"]
            if git(repo_dir, "cat-file", "-e", sha + "^{commit}", check=False).returncode == 0:
                return {sha: ""}
            return {sha: str(exc)[:300]}
        mid = len(refs) // 2
        return {**fetch_many(repo_dir, remote, refs[:mid]),
                **fetch_many(repo_dir, remote, refs[mid:])}


def path_kind(path):
    """Deliberately conservative: avoid calling arbitrary Rust files network code."""
    p = path.lower()
    parts = p.split("/")
    if ("tests" in parts or "testdata" in parts or "examples" in parts
            or p.endswith("_test.rs") or p.startswith("test/")):
        return "tests"
    if p.endswith(".rs"):
        if p.startswith(("iroh/src/", "iroh-net/src/", "iroh-relay/src/",
                         "iroh-dns/src/", "iroh-base/src/", "iroh-gossip/src/",
                         "noq/", "noq-proto/", "noq-udp/")):
            return "network"
        if "/src/" in p and any(w in parts for w in (
                "net", "network", "relay", "discovery", "transport", "quic",
                "socket", "routing", "p2p", "iroh")):
            return "network"
        if "identity" in p or "authentication" in p:
            return "identity"
        return "other_code"
    return "doc_config"


def changed_paths(repo_dir, base, sha):
    p = git(repo_dir, "diff", "--no-renames", "--name-only", base, sha,
            timeout=540)
    return [line for line in p.stdout.splitlines() if line]


def patch_id(repo_dir, base, sha, paths=None):
    """Stable patch-id, computed from full diff; never hash a filename-only summary."""
    if paths is not None and not paths:
        return ""
    cmd = ["git", "--git-dir=" + str(repo_dir), "diff", "--no-ext-diff",
           "--no-textconv", "--no-renames", "--binary", base, sha]
    if paths is not None:
        cmd += ["--", *paths]
    diff = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            env={**os.environ, "GIT_TERMINAL_PROMPT": "0"})
    try:
        result = subprocess.run(["git", "patch-id", "--stable"], stdin=diff.stdout,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, timeout=600)
    finally:
        diff.stdout.close()
        diff_return = diff.wait(timeout=600)
    if result.returncode or diff_return:
        raise RuntimeError("diff/patch-id failed, diff=%s id=%s" % (
            diff_return, result.stderr[-100:]))
    line = result.stdout.strip().splitlines()
    return line[0].split()[0] if line else ""


def write_csv(path, rows, fields):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def scan(shard, shards, commit_limit):
    if not 0 <= shard < shards or not 1 <= shards <= 16:
        raise RuntimeError("Invalid shard configuration")
    chosen = [r for r in read_candidates() if shard_for(r["sha"], shards) == shard]
    print("PATCH_AUDIT_START shard=%d/%d heads=%d" % (shard, shards, len(chosen)), flush=True)
    by_repo = defaultdict(list)
    for r in chosen:
        by_repo[r["representative_repo"]].append(r)
    commit_cache = {}
    branches, commits = [], []
    with tempfile.TemporaryDirectory(prefix="iroh-patch-audit-") as directory:
        repo_dir = Path(directory) / "objects.git"
        run("git", "clone", "--bare", "--filter=blob:none", "--no-tags",
            "--quiet", UPSTREAM, str(repo_dir), timeout=1200)
        baseline = "refs/heads/main"
        if git(repo_dir, "rev-list", "--count", baseline).stdout.strip() == "0":
            raise RuntimeError("Upstream history was not fetched")
        for i, (repo, refs) in enumerate(sorted(by_repo.items()), 1):
            fetch_status = {}
            for start in range(0, len(refs), 40):
                fetch_status.update(fetch_many(repo_dir, "https://github.com/" + repo + ".git",
                                               refs[start:start + 40]))
            for r in refs:
                sha = r["sha"]
                row = dict.fromkeys(BRANCH_FIELDS, "")
                row.update(sha=sha, repo=repo, branch=r["representative_branch"],
                           reference_count=r["references"], default_class=r["default_class"],
                           github_url="https://github.com/%s/tree/%s" % (repo, sha))
                try:
                    if fetch_status.get(sha):
                        row.update(status="fetch_failed", detail=fetch_status[sha])
                        branches.append(row)
                        continue
                    if git(repo_dir, "cat-file", "-e", sha + "^{commit}",
                           check=False).returncode:
                        row.update(status="commit_missing", detail="SHA unavailable after fetch")
                        branches.append(row)
                        continue
                    mb = git(repo_dir, "merge-base", baseline, sha, check=False)
                    if mb.returncode:
                        row.update(status="no_common_ancestor",
                                   detail=mb.stderr.strip()[-200:])
                        branches.append(row)
                        continue
                    base = mb.stdout.strip()
                    row["merge_base"] = base
                    row["ahead_commits"] = git(repo_dir, "rev-list", "--count",
                                               baseline + ".." + sha).stdout.strip()
                    paths = changed_paths(repo_dir, base, sha)
                    kinds = Counter(path_kind(x) for x in paths)
                    net_paths = [x for x in paths if path_kind(x) in ("network", "identity")]
                    row.update(
                        net_changed_files=str(len(paths)),
                        net_network_files=str(kinds["network"]),
                        net_identity_files=str(kinds["identity"]),
                        net_test_files=str(kinds["tests"]),
                        net_other_code_files=str(kinds["other_code"]),
                        net_doc_config_files=str(kinds["doc_config"]),
                        sample_changed_files=";".join(paths[:12]),
                    )
                    if len(paths) <= MAX_PATHS_FOR_PATCH:
                        row["net_patch_id"] = patch_id(repo_dir, base, sha)
                    if len(net_paths) <= MAX_PATHS_FOR_PATCH:
                        row["network_patch_id"] = patch_id(repo_dir, base, sha, net_paths)
                    else:
                        row["detail"] = "network path count exceeds patch limit"
                    fork_commits = git(repo_dir, "rev-list", "--no-merges",
                                       "--max-count=" + str(commit_limit + 1),
                                       baseline + ".." + sha).stdout.splitlines()
                    row["independent_commit_count"] = str(len(fork_commits)) if len(
                        fork_commits) <= commit_limit else ">=" + str(commit_limit + 1)
                    row["commit_detail_complete"] = str(len(fork_commits) <= commit_limit).lower()
                    for commit_sha in fork_commits[:commit_limit]:
                        if commit_sha in commit_cache:
                            continue
                        parent_line = git(repo_dir, "rev-list", "--parents", "-n", "1",
                                          commit_sha).stdout.split()
                        if len(parent_line) != 2:
                            continue
                        parent = parent_line[1]
                        cpaths = changed_paths(repo_dir, parent, commit_sha)
                        cnpaths = [p for p in cpaths if path_kind(p) in ("network", "identity")]
                        identity = Counter(path_kind(p) for p in cpaths)
                        commit_pid = patch_id(repo_dir, parent, commit_sha) if len(
                            cpaths) <= MAX_PATHS_FOR_PATCH else ""
                        network_pid = patch_id(repo_dir, parent, commit_sha, cnpaths) if len(
                            cnpaths) <= MAX_PATHS_FOR_PATCH else ""
                        subject = git(repo_dir, "log", "-1", "--pretty=%s",
                                      commit_sha).stdout.strip()
                        classification = "network" if identity["network"] else (
                            "identity" if identity["identity"] else (
                                "other_code" if identity["other_code"] else (
                                    "tests" if identity["tests"] else "docs_config")))
                        record = {"commit_sha": commit_sha, "patch_id": commit_pid,
                                  "network_patch_id": network_pid,
                                  "code_class": classification,
                                  "changed_files": len(cpaths), "subject": subject,
                                  "branch_head": sha, "repo": repo,
                                  "github_url": "https://github.com/%s/commit/%s" % (
                                      repo, commit_sha)}
                        commit_cache[commit_sha] = record
                        commits.append(record)
                    row["status"] = "audited" if len(paths) <= MAX_PATHS_FOR_PATCH else "large_diff"
                except Exception as exc:
                    row.update(status="analysis_error", detail=str(exc)[-300:])
                branches.append(row)
            if i % 15 == 0 or i == len(by_repo):
                print("PATCH_AUDIT_PROGRESS shard=%d repos=%d/%d rows=%d commits=%d" % (
                    shard, i, len(by_repo), len(branches), len(commits)), flush=True)
    write_csv(OUT / ("branches-%02d.csv" % shard), branches, BRANCH_FIELDS)
    write_csv(OUT / ("commits-%02d.csv" % shard), commits, COMMIT_FIELDS)
    stats = Counter(r["status"] for r in branches)
    print("PATCH_AUDIT_DONE", shard, dict(stats), "commit_rows", len(commits), flush=True)


def csv_rows(file):
    with file.open(encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def aggregate(shards):
    original = read_candidates()
    branches, commits = [], []
    for shard in range(shards):
        path = OUT / ("branches-%02d.csv" % shard)
        cpath = OUT / ("commits-%02d.csv" % shard)
        if not path.exists() or not cpath.exists():
            raise RuntimeError("Missing shard output: %s" % shard)
        branches.extend(csv_rows(path))
        commits.extend(csv_rows(cpath))
    expected = {r["sha"] for r in original}
    if {r["sha"] for r in branches} != expected or len(branches) != len(expected):
        raise RuntimeError("Incomplete branch audit; refusing success summary")
    # A SHA may be recorded in multiple shards' commit ancestry. Choose one row.
    by_commit = {c["commit_sha"]: c for c in commits}
    commits = list(by_commit.values())
    write_csv(ROOT / "patch_branch_audit.csv", branches, BRANCH_FIELDS)
    write_csv(ROOT / "patch_commit_index.csv", commits, COMMIT_FIELDS)
    pid_to_branches = defaultdict(list)
    net_to_branches = defaultdict(list)
    for b in branches:
        if b["net_patch_id"]:
            pid_to_branches[b["net_patch_id"]].append(b)
        if b["network_patch_id"]:
            net_to_branches[b["network_patch_id"]].append(b)
    group_rows = []
    for fingerprint, members in net_to_branches.items():
        if not fingerprint:
            continue
        members.sort(key=lambda r: (-int(r["net_network_files"] or 0),
                                    -int(r["net_identity_files"] or 0), r["repo"]))
        m = members[0]
        group_rows.append({
            "network_patch_id": fingerprint,
            "branch_heads": len(members),
            "distinct_repos": len({m["repo"] for m in members}),
            "representative_repo": m["repo"],
            "representative_branch": m["branch"],
            "representative_sha": m["sha"],
            "network_files": m["net_network_files"],
            "identity_files": m["net_identity_files"],
            "sample_files": m["sample_changed_files"],
            "member_shas": ";".join(r["sha"] for r in members[:20]),
        })
    group_rows.sort(key=lambda x: (-int(x["network_files"] or 0),
                                   -int(x["identity_files"] or 0),
                                   -int(x["branch_heads"]), x["representative_repo"]))
    group_fields = ["network_patch_id", "branch_heads", "distinct_repos",
                    "representative_repo", "representative_branch",
                    "representative_sha", "network_files", "identity_files",
                    "sample_files", "member_shas"]
    write_csv(ROOT / "patch_network_clusters.csv", group_rows, group_fields)
    commit_patchgroups = defaultdict(set)
    for c in commits:
        if c["patch_id"]:
            commit_patchgroups[c["patch_id"]].add(c["commit_sha"])
    stats = Counter(b["status"] for b in branches)
    duplicates = sum(len(v) - 1 for v in pid_to_branches.values() if len(v) > 1)
    netduplicates = sum(len(v) - 1 for v in net_to_branches.values() if len(v) > 1)
    commit_duplicates = sum(len(v) - 1 for v in commit_patchgroups.values()
                            if len(v) > 1)
    now = datetime.now(timezone.utc).isoformat()
    report = [
        "# СТАЯ — сверка патчей оставшихся 1762 ветвей Iroh",
        "", "Время расчёта: %s UTC" % now,
        "", "## Покрытие",
        "", "| Показатель | Значение |", "|---|---:|",
        "| Входных SHA ветвей | %d |" % len(original),
    ]
    for k in ["audited", "large_diff", "fetch_failed", "commit_missing",
              "no_common_ancestor", "analysis_error"]:
        report.append("| %s | %d |" % (k, stats[k]))
    report += [
        "| Отдельных SHA коммитов в подробной проверке | %d |" % len(commits),
        "| Разных нормализованных патчей коммитов (непустых) | %d |" % len(commit_patchgroups),
        "| Коммиты с совпадающим patch-id при другом SHA (сверх первого) | %d |" % commit_duplicates,
        "| Разных итоговых патчей ветвей | %d |" % len(pid_to_branches),
        "| Повторные итоговые патчи при разных SHA (сверх первого) | %d |" % duplicates,
        "| Разных итоговых патчей сетевого/identity кода | %d |" % len(net_to_branches),
        "| Повторные сетевые патчи при разных SHA (сверх первого) | %d |" % netduplicates,
        "", "## Метод и ограничения",
        "", "- Каждому SHA соответствует *вершина ветви*, а не обязательно единственный независимый коммит.",
        "- Для каждой доступной ветви получен общий предок с основным Iroh и вычислен `git patch-id --stable` от *итоговой разницы деревьев*. Это выявляет одинаковые изменения под разными SHA.",
        "- Индивидуальные patch-id коммитов собираются отдельно; подробная проверка ограничена 80 коммитами на голову. При превышении лимита `commit_detail_complete=false`.",
        "- Группы по сетевому коду вычисляются без документации и тестов. Это *синтаксическое совпадение* — не доказательство эквивалентности поведения, пригодности или наличия тестов.",
        "- Сложные/большие изменения и недоступные форки не исключать автоматически. Совпадение patch-id не гарантирует одинаковое поведение на разных версиях Iroh/noq.",
        "- Файлы: patch_branch_audit.csv, patch_commit_index.csv, patch_network_clusters.csv.",
        "", "## Приоритетные различия сетевого кода (файлы, не подтверждённые функции)",
        "", "| Представитель | Ветка | Сетевых .rs | Совпадений |",
        "|---|---|---:|---:|",
    ]
    for g in group_rows[:35]:
        report.append("| [%s](https://github.com/%s) | %s | %s | %s |" % (
            g["representative_repo"], g["representative_repo"],
            g["representative_branch"].replace("|", "/"),
            g["network_files"], g["branch_heads"]))
    report += [
        "", "Следующий шаг: ручная проверка top-35 сетевых изменений, регрессионные тесты Roha/noq и лицензионная проверка. Ничего не сливать автоматически."
    ]
    (ROOT / "PATCH-SUMMARY.md").write_text("\n".join(report) + "\n", encoding="utf-8")
    print("PATCH_AGGREGATE", dict(stats), "commit_duplicates", commit_duplicates,
          "network_fingerprints", len(net_to_branches), flush=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--shard", type=int, default=None)
    p.add_argument("--shards", type=int, default=SHARD_COUNT)
    p.add_argument("--aggregate", action="store_true")
    p.add_argument("--commit-limit", type=int, default=MAX_COMMIT_DETAILS)
    args = p.parse_args()
    if args.aggregate:
        aggregate(args.shards)
    else:
        if args.shard is None:
            p.error("--shard required for scan")
        scan(args.shard, args.shards, args.commit_limit)


if __name__ == "__main__":
    main()
#!/usr/bin/env python3
"""Census all officially listed n0-computer/iroh forks on GitHub.

Default-branch differences are not proof of the absence of other branches.
"""
import concurrent.futures
import csv
import datetime
import json
import os
import pathlib
import random
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter

ROOT = "n0-computer/iroh"
DATE = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d")
OUT = pathlib.Path("docs/staya/iroh-fork-census") / DATE
TOKEN = os.environ.get("GH_TOKEN", "").strip()
HEADERS = {
    "Accept": "application/vnd.github+json",
    "User-Agent": "staya-iroh-fork-census/1.0",
    "X-GitHub-Api-Version": "2022-11-28",
    "Authorization": "Bearer " + TOKEN,
}


def api(path):
    for attempt in range(5):
        try:
            req = urllib.request.Request(
                "https://api.github.com" + path, headers=HEADERS
            )
            with urllib.request.urlopen(req, timeout=45) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            body = exc.read(400).decode("utf-8", "replace")
            remaining = exc.headers.get("X-RateLimit-Remaining")
            if exc.code in (403, 429, 502, 503, 504) and attempt < 4:
                reset = exc.headers.get("X-RateLimit-Reset")
                delay = (
                    min(90, max(2, int(reset) - int(time.time()) + 2))
                    if remaining == "0" and reset
                    else min(45, 3 * (2 ** attempt)) + random.random()
                )
                print("RETRY", exc.code, "sleep", round(delay), path[:110],
                      flush=True)
                time.sleep(delay)
                continue
            raise RuntimeError(
                "GitHub HTTP %s on %s: %s" % (exc.code, path, body)
            ) from exc
        except (TimeoutError, urllib.error.URLError):
            if attempt == 4:
                raise
            time.sleep(3 * (attempt + 1))
    raise RuntimeError("request retries exhausted: " + path)


def enumerate_forks():
    found = {}
    for page in range(1, 51):
        batch = api(
            "/repos/%s/forks?per_page=100&sort=newest&page=%s"
            % (ROOT, page)
        )
        if not isinstance(batch, list):
            raise RuntimeError("Bad official fork response")
        for repo in batch:
            if repo.get("id") and repo.get("full_name"):
                found[repo["id"]] = repo
        print("FORK_PAGE", page, "count", len(batch),
              "unique", len(found), flush=True)
        if len(batch) < 100:
            return list(found.values()), page
    raise RuntimeError("More than 50 pages; refusing incomplete claim")


def classify(fork):
    name = fork["full_name"]
    owner = fork["owner"]["login"]
    branch = fork.get("default_branch") or "main"
    ref = urllib.parse.quote(owner + ":" + branch, safe=":")
    row = {
        "repo_id": fork["id"], "full_name": name, "url": fork["html_url"],
        "default_branch": branch, "pushed_at": fork.get("pushed_at", ""),
        "archived": bool(fork.get("archived", False)),
        "size_kb": fork.get("size"), "forks_count": fork.get("forks_count", 0),
        "ahead_by": None, "behind_by": None, "changed_files": 0,
        "production_files": 0, "tests_files": 0, "classification": "unknown",
        "examples": "", "error": "", "other_branches_not_examined": True,
    }
    try:
        data = api(
            "/repos/%s/compare/main...%s?per_page=1" % (ROOT, ref)
        )
        row["ahead_by"] = data.get("ahead_by")
        row["behind_by"] = data.get("behind_by")
        files = [item.get("filename", "") for item in data.get("files", [])]
        prod = [
            file for file in files if re.search(
                r"^(iroh|iroh-relay|iroh-base|iroh-dns|iroh-dns-server|"
                r"iroh-metrics|iroh-net-report|iroh-net|iroh-sockets)"
                r"/src/.+\.rs$", file
            )
        ]
        tests = [
            file for file in files if "/tests/" in file
            or re.search(r"(^|/)test[^/]*\.rs$", file)
        ]
        row.update({
            "changed_files": len(files), "production_files": len(prod),
            "tests_files": len(tests),
            "examples": ";".join(list(dict.fromkeys(prod + tests + files))[:8]),
        })
        ahead = row["ahead_by"]
        if ahead == 0:
            row["classification"] = (
                "main_identical" if row["behind_by"] == 0 else "main_outdated"
            )
        elif ahead is None:
            row["classification"] = "unknown"
        elif prod:
            row["classification"] = "own_network_code"
        elif tests:
            row["classification"] = "tests_and_examples"
        else:
            row["classification"] = "configs_docs_other"
    except Exception as exc:
        row["error"] = str(exc)[:350]
    return row


def save(rows, forks, page_count, upstream):
    OUT.mkdir(parents=True, exist_ok=True)
    fields = [
        "repo_id", "full_name", "url", "default_branch", "classification",
        "ahead_by", "behind_by", "production_files", "tests_files",
        "changed_files", "pushed_at", "archived", "size_kb", "forks_count",
        "other_branches_not_examined", "examples", "error",
    ]
    weights = {
        "own_network_code": 0, "tests_and_examples": 1,
        "configs_docs_other": 2, "unknown": 3,
        "main_outdated": 4, "main_identical": 5,
    }
    rows.sort(key=lambda row: (
        weights.get(row["classification"], 6),
        -(row["production_files"] or 0),
        -(row["ahead_by"] or 0),
        row["full_name"].lower(),
    ))
    with (OUT / "all_forks.csv").open(
        "w", encoding="utf-8", newline=""
    ) as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    with (OUT / "network_code_candidates.csv").open(
        "w", encoding="utf-8", newline=""
    ) as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(
            row for row in rows
            if row["classification"] == "own_network_code"
        )
    with (OUT / "all_forks.json").open("w", encoding="utf-8") as stream:
        json.dump({
            "date": DATE, "upstream": ROOT,
            "github_forks_count": upstream.get("forks_count"),
            "officially_listed_forks": len(forks),
            "rows": rows,
        }, stream, ensure_ascii=False, indent=2)

    counter = Counter(row["classification"] for row in rows)
    expected = upstream.get("forks_count")
    summary = [
        "# СТАЯ — первичный отсев форков Iroh",
        "",
        "**Дата снимка:** " + DATE,
        "**Метод:** официальный список GitHub REST /repos/n0-computer/iroh/forks,",
        "сравнение default-ветки через REST compare с исходным main.",
        "",
        "- Счётчик GitHub: **%s**." % expected,
        "- Реально перечислено уникальных репозиториев: **%d** на %d страницах."
        % (len(forks), page_count),
        "- Разница со счётчиком: **%d** (может меняться по ходу обхода,",
        "  включать недоступные репозитории)."
        % (expected - len(forks)),
        "- Ошибок/неопределённостей сравнения: **%d**."
        % counter["unknown"],
        "- **Важно:** дополнительные ветки ещё не проверены. Зеркало на",
        "  default-ветке НЕ означает, что весь форк бесполезен.",
        "",
        "## Первичная классификация",
        "",
        "| Категория | Количество | Действие |",
        "|---|---:|---|",
        "| Свои изменения сетевого кода | %d | Приоритет на изучение |"
        % counter["own_network_code"],
        "| Дополнительные тесты/примеры | %d | Изучить сценарии |"
        % counter["tests_and_examples"],
        "| Только прочие файлы | %d | Низкий приоритет |"
        % counter["configs_docs_other"],
        "| Отставшая основная ветка | %d | Отложить main; осмотреть ветки |"
        % counter["main_outdated"],
        "| Основная ветка идентична | %d | Отложить main; осмотреть ветки |"
        % counter["main_identical"],
        "| Не удалось сравнить | %d | Повторить |"
        % counter["unknown"],
        "",
        "## Основные ветки с сетевыми изменениями",
        "",
        "| Форк | Впереди upstream | Производственных файлов | pushed |",
        "|---|---:|---:|---|",
    ]
    ranked = [r for r in rows if r["classification"] == "own_network_code"]
    for row in ranked[:120]:
        summary.append(
            "| [%s](%s) | %s | %s | %s |"
            % (row["full_name"], row["url"], row["ahead_by"],
               row["production_files"], (row["pushed_at"] or "")[:10])
        )
    summary += [
        "",
        "## Границы доказательств",
        "",
        "1. Данные all_forks.csv охватывают только официально перечисленные",
        "   доступные репозитории. Сравнения используются для первой сортировки.",
        "2. Отдельные ветки fix/*, feat/*, потомков форков ещё требуют прохода.",
        "3. Патчи более 300 файлов могут быть обрезаны GitHub compare.",
        "4. Любое заимствование — только после чтения diff, сравнения с",
        "   нашим Roha, независимого теста и проверки лицензии.",
        "",
        "Машинный перечень: all_forks.csv и all_forks.json; основной отбор:",
        "network_code_candidates.csv.",
    ]
    (OUT / "SUMMARY.md").write_text(
        "\n".join(summary) + "\n", encoding="utf-8"
    )
    print("CENSUS_RESULT", json.dumps({
        "date": DATE, "github_count": expected,
        "listed": len(forks), "pages": page_count, "classes": dict(counter),
        "top": [x["full_name"] for x in ranked[:25]],
    }, ensure_ascii=False), flush=True)


def main():
    if not TOKEN:
        raise SystemExit("GH_TOKEN is required")
    upstream = api("/repos/" + ROOT)
    forks, pages = enumerate_forks()
    if not forks:
        raise RuntimeError("No official forks; abort")
    rows = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
        for row in pool.map(classify, forks):
            rows.append(row)
            if len(rows) % 50 == 0 or len(rows) == len(forks):
                classes = Counter(x["classification"] for x in rows)
                print("COMPARED", len(rows), "of", len(forks),
                      dict(classes), flush=True)
    save(rows, forks, pages, upstream)


if __name__ == "__main__":
    main()

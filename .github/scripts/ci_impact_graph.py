#!/usr/bin/env python3
"""Read-only Cargo workspace -> reverse dependencies -> PR workflow fanout audit.

This is a conservative audit, NOT an automatic permission to skip CI jobs.
Requires Python 3.11+. No third-party modules or GitHub token.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict, deque
import json
from pathlib import Path
import re
import statistics
import subprocess
import tomllib


def load_toml(path: Path) -> dict:
    with path.open('rb') as stream:
        return tomllib.load(stream)


def workspace(root: Path) -> tuple[dict, dict, dict]:
    root = root.resolve()
    top = load_toml(root / 'Cargo.toml')
    members = top.get('workspace', {}).get('members', [])
    if not members:
        raise ValueError(f'no workspace.members found in {root / "Cargo.toml"}')
    packages = {}
    locations = {}
    manifests = {}
    for member in members:
        paths = sorted(root.glob(str(member)))
        if not paths:
            raise ValueError(f'missing workspace member: {member}')
        for folder in paths:
            manifest = folder / 'Cargo.toml'
            if not manifest.is_file():
                raise ValueError(f'missing manifest for workspace member: {member}')
            item = load_toml(manifest)
            name = item['package']['name']
            if name in packages:
                raise ValueError(f'duplicate workspace package: {name}')
            packages[name] = str(folder.relative_to(root))
            locations[folder.resolve()] = name
            manifests[name] = item
    # Keep runtime and test-only edges separate.
    dependencies = {}
    root_deps = top.get('workspace', {}).get('dependencies', {})
    for name, item in manifests.items():
        parent = (root / packages[name]).resolve()
        edges = {}
        tables = [item]
        tables += [value for value in item.get('target', {}).values() if isinstance(value, dict)]
        for table in tables:
            for section in ('dependencies', 'build-dependencies', 'dev-dependencies'):
                for alias, spec in table.get(section, {}).items():
                    if not isinstance(spec, dict):
                        continue
                    if spec.get('workspace') is True:
                        inherited = root_deps.get(alias, {})
                        if isinstance(inherited, dict):
                            spec = {**inherited, **spec}
                    path = spec.get('path')
                    if not path:
                        continue
                    target = locations.get((parent / path).resolve())
                    if not target:
                        continue  # outside the local Cargo workspace
                    kind = 'test-only' if section == 'dev-dependencies' else 'runtime'
                    if edges.get(target) != 'runtime':
                        edges[target] = kind
        dependencies[name] = edges
    return packages, dependencies, manifests


def dependents(start: set[str], dependencies: dict) -> dict[str, str]:
    reverse = defaultdict(list)
    for package, edges in dependencies.items():
        for dep, kind in edges.items():
            reverse[dep].append((package, kind))
    impact = {name: 'runtime' for name in start}
    queue = deque(start)
    while queue:
        item = queue.popleft()
        for downstream, edge_kind in reverse[item]:
            kind = ('runtime' if impact[item] == 'runtime' and edge_kind == 'runtime'
                    else 'test-only')
            previous = impact.get(downstream)
            if previous is None or (kind == 'runtime' and previous == 'test-only'):
                impact[downstream] = kind
                queue.append(downstream)
    return dict(sorted(impact.items()))


def file_package(path: str, packages: dict) -> str | None:
    # Longest prefix wins for nested workspace members.
    matches = [(len(folder), name) for name, folder in packages.items()
               if path == folder or path.startswith(folder.rstrip('/') + '/')]
    return max(matches)[1] if matches else None


def tracked_files(root: Path) -> list[str]:
    completed = subprocess.run(['git', '-C', str(root), 'ls-files', '-z'],
                               capture_output=True, check=False)
    if completed.returncode == 0:
        return sorted(x.decode('utf-8', 'surrogateescape')
                      for x in completed.stdout.split(b'\0') if x)
    return sorted(str(p.relative_to(root)) for p in root.rglob('*.rs'))


def changed_file_frequency(root: Path, limit: int) -> Counter:
    completed = subprocess.run(['git', '-C', str(root), 'log', '--first-parent',
                                f'-n{limit}', '--format=@@COMMIT@@', '--name-only',
                                '--diff-filter=ACMR'], capture_output=True,
                               text=True, check=False)
    if completed.returncode:
        return Counter()
    counts = Counter()
    per_commit = set()
    for line in completed.stdout.splitlines() + ['@@COMMIT@@']:
        line = line.strip()
        if line == '@@COMMIT@@':
            counts.update(per_commit)
            per_commit.clear()
        elif line:
            per_commit.add(line)
    return counts


def pr_workflows(root: Path) -> list[dict]:
    result = []
    for f in sorted((root / '.github' / 'workflows').glob('*')):
        if f.suffix not in ('.yaml', '.yml') or not f.is_file():
            continue
        lines = f.read_text('utf-8').splitlines()
        in_on = False
        events = set()
        filter_lines = []
        for line in lines:
            if re.match(r'^on:\s*$', line):
                in_on = True
                continue
            inline = re.match(r'^on:\s*\[(.*?)\]', line)
            if inline:
                events.update(x.strip() for x in inline.group(1).split(','))
                break
            if in_on:
                if line and not line.startswith((' ', '#')):
                    break
                match = re.match(r'^  ([a-z_]+):\s*', line)
                if match:
                    events.add(match.group(1))
                if re.match(r'^\s{4,}(paths|paths-ignore):', line):
                    filter_lines.append(line.strip())
        if not (events & {'pull_request', 'pull_request_target'}):
            continue
        # Exact GitHub glob and job-level if semantics are not inferred here.
        result.append({'workflow': str(f.relative_to(root)),
                       'scope': 'conditional-paths' if filter_lines else 'unfiltered',
                       'path_filters': filter_lines})
    return result


def workflow_cost(jobs_paths: list[Path]) -> list[dict]:
    durations = defaultdict(list)
    for path in jobs_paths:
        payload = json.loads(path.read_text('utf-8'))
        for job in payload.get('jobs', []):
            if job.get('conclusion') not in ('success', 'failure'):
                continue
            start, end = job.get('started_at'), job.get('completed_at')
            if not start or not end:
                continue
            from datetime import datetime
            try:
                a = datetime.fromisoformat(start.replace('Z', '+00:00'))
                b = datetime.fromisoformat(end.replace('Z', '+00:00'))
                seconds = (b - a).total_seconds()
            except (ValueError, TypeError):
                continue
            if 0 <= seconds < 86400:
                durations[job['name']].append(seconds / 60)
    return sorted(({'job': name, 'samples': len(times),
                    'median_minutes': round(statistics.median(times), 2),
                    'max_minutes': round(max(times), 2)}
                   for name, times in durations.items()),
                  key=lambda x: -x['median_minutes'])


def report(root: Path, limit: int, top: int, changed: list[str], costs: list[Path]) -> dict:
    packages, deps, _ = workspace(root)
    files = [name for name in tracked_files(root) if name.endswith('.rs')]
    frequency = changed_file_frequency(root, limit)
    records = []
    for name in files:
        path = root / name
        if not path.is_file():
            continue
        pkg = file_package(name, packages)
        if pkg is None:
            continue
        with path.open('rb') as stream:
            lines = sum(1 for _ in stream)
        records.append({'file': name, 'lines': lines,
                        'bytes': path.stat().st_size, 'changed_commits': frequency[name],
                        'crate': pkg, 'affected': dependents({pkg}, deps)})
    records.sort(key=lambda row: (-row['changed_commits'], -row['bytes'], row['file']))
    workflows = pr_workflows(root)
    selected = {}
    for item in changed:
        item = item.replace('\\', '/').removeprefix('./')
        pkg = file_package(item, packages)
        force_full = (item in ('Cargo.toml', 'Cargo.lock')
                      or item.startswith('.github/workflows/'))
        if item.endswith('/Cargo.toml') and pkg:
            force_full = True
        selected[item] = {'crate': pkg,
                          'affected': dependents({pkg}, deps) if pkg else {},
                          'requires_full_gate': force_full,
                          'note': 'Workflow candidates, not authorization to skip any check.'}
    return {'schema': 1, 'repository': root.name, 'workspace_members': packages,
            'dependency_edges': deps, 'files_mapped': len(records),
            'history_first_parent_commits': limit,
            'workflows': workflows, 'top_files': records[:top],
            'selected_files': selected, 'job_timings': workflow_cost(costs),
            'safety': 'REPORT ONLY: Do not use this as a CI skip/merge gate. '
                      'Dev, feature, target, generated and external dependencies '
                      'need exact Cargo CI validation.'}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path('.'))
    parser.add_argument('--history', type=int, default=30, help='first-parent commits')
    parser.add_argument('--top', type=int, default=20)
    parser.add_argument('--changed', action='append', default=[], help='file to inspect')
    parser.add_argument('--jobs-json', action='append', type=Path, default=[],
                        help='GitHub Actions jobs API JSON')
    args = parser.parse_args()
    try:
        data = report(args.repo.resolve(), args.history, args.top, args.changed, args.jobs_json)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as exc:
        parser.error(str(exc))
    print(json.dumps(data, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()

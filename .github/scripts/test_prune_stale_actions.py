"""Regression tests for safe cancellation of GitHub Actions queue entries."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import unittest

os.environ.setdefault("GITHUB_REPOSITORY", "test/roha")
os.environ.setdefault("GH_TOKEN", "test-token-unused")
os.environ.setdefault("GITHUB_RUN_ID", "1")

module_path = Path(__file__).with_name("prune-stale-actions.py")
spec = importlib.util.spec_from_file_location("prune_stale_actions", module_path)
assert spec is not None and spec.loader is not None
pruner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pruner)


class StaleRunPolicyTests(unittest.TestCase):
    def make_run(self, path: str, event: str, sha: str = "old") -> dict:
        return {
            "id": 2,
            "path": path,
            "event": event,
            "head_branch": "main" if event == "push" else "work",
            "head_sha": sha,
            "created_at": pruner.NOW.isoformat(),
        }

    def reason(self, run: dict, active: set[str] | None = None) -> str | None:
        return pruner.choose_reason(run, active or set(), "current-main", "main")

    def test_merged_pr_branch_hygiene_must_survive(self):
        run = self.make_run(".github/workflows/branch-hygiene.yml", "pull_request")
        self.assertIsNone(self.reason(run))

    def test_archive_survives_main_advancing(self):
        run = self.make_run(
            ".github/workflows/archive-verified-legacy-branches.yml", "push"
        )
        self.assertIsNone(self.reason(run))

    def test_closed_pr_regular_ci_is_still_cancelled(self):
        run = self.make_run(".github/workflows/ci.yml", "pull_request")
        self.assertEqual(
            self.reason(run), "PR closed or head SHA superseded"
        )

    def test_superseded_main_ci_is_still_cancelled(self):
        run = self.make_run(".github/workflows/ci.yml", "push")
        self.assertEqual(self.reason(run), "superseded main commit")

    def test_active_pr_ci_is_preserved(self):
        run = self.make_run(".github/workflows/ci.yml", "pull_request", "active")
        self.assertIsNone(self.reason(run, {"active"}))

    def test_spoofed_name_does_not_exempt_untrusted_path(self):
        run = self.make_run(".github/workflows/ci.yml", "pull_request")
        run["name"] = "Branch hygiene"
        self.assertEqual(
            self.reason(run), "PR closed or head SHA superseded"
        )


if __name__ == "__main__":
    unittest.main()

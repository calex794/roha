"""Regression tests for the safety boundary between queue pruning and ref cleanup."""
import datetime as dt
import importlib.util
import os
from pathlib import Path
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "prune-stale-actions.py"
with mock.patch.dict(os.environ, {
    "GITHUB_REPOSITORY": "calex794/roha",
    "GH_TOKEN": "unit-test-token",
    "GITHUB_RUN_ID": "999",
}):
    spec = importlib.util.spec_from_file_location("prune_stale_actions", SCRIPT)
    pruner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pruner)


def make_run(run_id, path, event="pull_request", branch="merged-pr", sha="closed-sha"):
    return {
        "id": run_id,
        "path": path,
        "event": event,
        "head_branch": branch,
        "head_sha": sha,
        "name": "Arbitrary display name",
        "status": "queued",
        "created_at": (pruner.NOW - dt.timedelta(minutes=90)).isoformat(),
    }


class RefMaintenanceSafetyTests(unittest.TestCase):
    def test_closed_merged_pr_branch_hygiene_is_never_pruned(self):
        run = make_run(10, pruner.BRANCH_HYGIENE_PATH)
        self.assertIsNone(pruner.choose_reason(run, set(), "main-sha", "main"))

    def test_ref_suffix_is_supported(self):
        run = make_run(11, pruner.BRANCH_HYGIENE_PATH + "@refs/heads/main")
        self.assertIsNone(pruner.choose_reason(run, set(), "main-sha", "main"))

    def test_unrelated_closed_pr_ci_is_still_pruned(self):
        run = make_run(12, ".github/workflows/ci.yml")
        self.assertEqual(
            pruner.choose_reason(run, set(), "main-sha", "main"),
            "PR closed or head SHA superseded",
        )

    def test_similarly_named_or_display_named_workflow_is_not_exempt(self):
        run = make_run(13, pruner.BRANCH_HYGIENE_PATH + ".backup")
        run["name"] = "Branch hygiene"
        self.assertEqual(
            pruner.choose_reason(run, set(), "main-sha", "main"),
            "PR closed or head SHA superseded",
        )

    def test_branch_hygiene_wrong_event_is_not_exempt(self):
        run = make_run(14, pruner.BRANCH_HYGIENE_PATH, event="push", branch="main", sha="old-main")
        self.assertEqual(
            pruner.choose_reason(run, set(), "main-sha", "main"),
            "superseded main commit",
        )

    def test_one_shot_archive_survives_newer_main_commit(self):
        run = make_run(15, pruner.LEGACY_ARCHIVE_PATH, event="push", branch="main", sha="old-main")
        self.assertIsNone(pruner.choose_reason(run, set(), "main-sha", "main"))

    def test_archive_from_nondefault_branch_is_not_exempt(self):
        run = make_run(16, pruner.LEGACY_ARCHIVE_PATH, event="push", branch="feature", sha="old-main")
        self.assertFalse(pruner.is_ref_maintenance_run(run, "main"))

    def test_open_pr_orphan_ci_still_runs(self):
        run = make_run(17, ".github/workflows/ci.yml")
        with mock.patch.object(pruner, "only_unavailable_self_hosted_jobs", return_value=False) as check:
            self.assertIsNone(pruner.choose_reason(run, {"closed-sha"}, "main-sha", "main"))
        check.assert_called_once_with(17)

    def test_missing_workflow_path_does_not_trust_display_name(self):
        run = make_run(18, None)
        run["name"] = "Branch hygiene"
        self.assertEqual(
            pruner.choose_reason(run, set(), "main-sha", "main"),
            "PR closed or head SHA superseded",
        )

    def test_main_preserves_merged_cleanup_but_cancels_obsolete_ci(self):
        maintenance = make_run(20, pruner.BRANCH_HYGIENE_PATH)
        stale_ci = make_run(21, ".github/workflows/ci.yml")
        calls = []

        def fake_list_pages(path, result_key=None):
            if "/pulls?state=open" in path:
                return []
            if "/actions/runs?status=queued" in path:
                return [maintenance, stale_ci]
            return []

        def fake_request(path, method="GET"):
            if method == "POST":
                calls.append(path)
                return {}
            if path == "/repos/calex794/roha":
                return {"default_branch": "main"}
            if path.endswith("/git/ref/heads/main"):
                return {"object": {"sha": "main-sha"}}
            if path.endswith("/actions/runs/20"):
                return maintenance
            if path.endswith("/actions/runs/21"):
                return stale_ci
            raise AssertionError(f"Unexpected request: {method} {path}")

        with mock.patch.object(pruner, "list_pages", side_effect=fake_list_pages), \
             mock.patch.object(pruner, "request", side_effect=fake_request), \
             mock.patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": ""}):
            self.assertEqual(pruner.main(), 0)
        self.assertEqual(calls, ["/repos/calex794/roha/actions/runs/21/cancel"])

    def test_recheck_live_run_metadata_before_cancellation(self):
        stale_ci = make_run(30, ".github/workflows/ci.yml")
        now_protected = make_run(30, pruner.BRANCH_HYGIENE_PATH)
        calls = []

        def fake_list_pages(path, result_key=None):
            if "/pulls?state=open" in path:
                return []
            if "/actions/runs?status=queued" in path:
                return [stale_ci]
            return []

        def fake_request(path, method="GET"):
            if method == "POST":
                calls.append(path)
                return {}
            if path == "/repos/calex794/roha":
                return {"default_branch": "main"}
            if path.endswith("/git/ref/heads/main"):
                return {"object": {"sha": "main-sha"}}
            if path.endswith("/actions/runs/30"):
                return now_protected
            raise AssertionError(f"Unexpected request: {method} {path}")

        with mock.patch.object(pruner, "list_pages", side_effect=fake_list_pages), \
             mock.patch.object(pruner, "request", side_effect=fake_request), \
             mock.patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": ""}):
            self.assertEqual(pruner.main(), 0)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()

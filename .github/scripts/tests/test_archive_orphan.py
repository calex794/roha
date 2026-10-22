"""Safe, offline regression tests for the actual archive-orphan.sh helper."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

HELPER = Path(__file__).resolve().parents[1] / "archive-orphan.sh"
GOOD_SHA = "a" * 40
BAD_SHA = "b" * 40

FAKE_GH = textwrap.dedent(r"""
    #!/usr/bin/env python3
    import json
    import os
    from pathlib import Path
    import sys

    args = sys.argv[1:]
    log = Path(os.environ["FAKE_ACTION_LOG"])
    tag_file = Path(os.environ["FAKE_TAG_FILE"])

    def record(item):
        with log.open("a", encoding="utf-8") as out:
            out.write(item + "\n")

    if args[:2] == ["auth", "setup-git"]:
        sys.exit(0)
    if not args or args[0] != "api":
        sys.exit(70)
    if "--paginate" in args:
        if os.environ.get("FAKE_OPEN_BASE"):
            print(os.environ["FAKE_OPEN_BASE"] + "\twork\tcalex794/roha")
        sys.exit(0)
    if "--method" in args and "POST" in args:
        tag_file.write_text(os.environ["EXPECTED_SHA"], encoding="ascii")
        record("tag-created")
        sys.exit(0)

    path = next((value for value in args if value.startswith("repos/")), "")
    if "/git/ref/heads/" in path:
        print(os.environ["EXPECTED_SHA"])
    elif "/branches/" in path:
        print(os.environ.get("FAKE_PROTECTED", "false"))
    elif "/git/matching-refs/tags/" in path:
        mode = os.environ["FAKE_TAG_MODE"]
        if mode == "api-error":
            print("temporary API failure", file=sys.stderr)
            sys.exit(22)
        if mode == "malformed":
            print(json.dumps({"message": "not a tag list"}))
            sys.exit(0)

        suffix = path.split("/git/matching-refs/tags/", 1)[1]
        full_ref = "refs/tags/" + suffix
        if tag_file.exists():
            target = tag_file.read_text(encoding="ascii")
        elif mode == "same":
            target = os.environ["EXPECTED_SHA"]
        elif mode == "conflict":
            target = "b" * 40
        elif mode == "lookalike":
            print(json.dumps([{"ref": full_ref + "-suffix",
                               "object": {"sha": os.environ["EXPECTED_SHA"]}}]))
            sys.exit(0)
        else:
            print("[]")
            sys.exit(0)
        print(json.dumps([{"ref": full_ref, "object": {"sha": target}}]))
    else:
        print("unexpected gh API operation: " + repr(args), file=sys.stderr)
        sys.exit(70)
""").lstrip()

FAKE_GIT = textwrap.dedent(r"""
    #!/usr/bin/env python3
    import os
    from pathlib import Path
    import sys
    if sys.argv[1:2] == ["push"]:
        with Path(os.environ["FAKE_ACTION_LOG"]).open("a", encoding="utf-8") as out:
            out.write("branch-deleted\n")
    sys.exit(0)
""").lstrip()


class ArchiveOrphanSafetyTests(unittest.TestCase):
    def run_helper(self, tag_mode="missing", mode="DRY_RUN", extra=None):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        binaries = root / "bin"
        binaries.mkdir()
        for name, content in (("gh", FAKE_GH), ("git", FAKE_GIT)):
            binary = binaries / name
            binary.write_text(content, encoding="utf-8")
            binary.chmod(0o755)
        summary = root / "summary.txt"
        summary.touch()
        action_log = root / "actions.txt"
        tag_file = root / "tag.txt"
        env = dict(os.environ)
        env.update({
            "PATH": str(binaries) + os.pathsep + env["PATH"],
            "GH_TOKEN": "test-only-no-real-token",
            "REPO": "calex794/roha",
            "TARGET_BRANCH": "ci/test-archive",
            "EXPECTED_SHA": GOOD_SHA,
            "DEFAULT_BRANCH": "main",
            "MODE": mode,
            "CONFIRMATION": "ARCHIVE",
            "GITHUB_STEP_SUMMARY": str(summary),
            "TMPDIR": str(root),
            "FAKE_ACTION_LOG": str(action_log),
            "FAKE_TAG_FILE": str(tag_file),
            "FAKE_TAG_MODE": tag_mode,
        })
        env.update(extra or {})
        finished = subprocess.run(
            ["bash", str(HELPER)], env=env, cwd=root, capture_output=True,
            text=True, timeout=15, check=False,
        )
        actions = action_log.read_text(encoding="utf-8").splitlines() if action_log.exists() else []
        return finished, actions, summary.read_text(encoding="utf-8")

    def test_absent_tag_dry_run_is_allowed_without_changes(self):
        run, actions, summary = self.run_helper()
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertIn("DRY RUN", summary)
        self.assertEqual(actions, [])

    def test_matching_tag_dry_run_is_allowed(self):
        run, actions, _ = self.run_helper(tag_mode="same")
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertEqual(actions, [])

    def test_conflicting_tag_never_deletes_branch(self):
        run, actions, _ = self.run_helper(tag_mode="conflict", mode="ARCHIVE")
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("different commit", run.stdout + run.stderr)
        self.assertEqual(actions, [])

    def test_absent_tag_is_created_and_verified_before_deletion(self):
        run, actions, summary = self.run_helper(mode="ARCHIVE")
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertEqual(actions, ["tag-created", "branch-deleted"])
        self.assertIn("Archived ci/test-archive", summary)

    def test_existing_exact_tag_may_be_reused(self):
        run, actions, _ = self.run_helper(tag_mode="same", mode="ARCHIVE")
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertEqual(actions, ["branch-deleted"])

    def test_prefix_similar_ref_is_not_an_exact_match(self):
        run, actions, _ = self.run_helper(tag_mode="lookalike", mode="ARCHIVE")
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertEqual(actions, ["tag-created", "branch-deleted"])

    def test_api_error_fails_closed(self):
        run, actions, _ = self.run_helper(tag_mode="api-error", mode="ARCHIVE")
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("Unable to list archive tags", run.stdout + run.stderr)
        self.assertEqual(actions, [])

    def test_malformed_api_response_fails_closed(self):
        run, actions, _ = self.run_helper(tag_mode="malformed", mode="ARCHIVE")
        self.assertNotEqual(run.returncode, 0)
        self.assertEqual(actions, [])

    def test_open_pr_base_blocks_archival(self):
        run, actions, _ = self.run_helper(mode="ARCHIVE", extra={"FAKE_OPEN_BASE": "ci/test-archive"})
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("An open PR uses this branch", run.stdout + run.stderr)
        self.assertEqual(actions, [])

    def test_protected_branch_blocks_archival(self):
        run, actions, _ = self.run_helper(mode="ARCHIVE", extra={"FAKE_PROTECTED": "true"})
        self.assertNotEqual(run.returncode, 0)
        self.assertEqual(actions, [])


if __name__ == "__main__":
    unittest.main()

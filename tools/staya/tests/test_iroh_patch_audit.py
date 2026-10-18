"""Offline tests for branch-diff equivalence and conservative categorization."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "iroh_patch_audit.py"
spec = importlib.util.spec_from_file_location("iroh_patch_audit", SCRIPT)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def git(directory, *args):
    p = subprocess.run(["git", *args], cwd=directory, capture_output=True, text=True)
    if p.returncode:
        raise AssertionError(p.stderr)
    return p.stdout.strip()


class PatchAuditTests(unittest.TestCase):
    def test_paths_not_confused_with_docs_or_tests(self):
        self.assertEqual(audit.path_kind("iroh/src/socket/transports.rs"), "network")
        self.assertEqual(audit.path_kind("iroh/tests/path_tests.rs"), "tests")
        self.assertEqual(audit.path_kind("crates/krikos-identity/src/lib.rs"), "identity")
        self.assertEqual(audit.path_kind("Cargo.lock"), "doc_config")
        self.assertEqual(audit.path_kind("src/unrelated.rs"), "other_code")

    def test_equivalent_patch_ignores_commit_identity(self):
        with tempfile.TemporaryDirectory() as t:
            cwd = Path(t)
            git(cwd, "init", "-q", "-b", "main")
            git(cwd, "config", "user.name", "Test")
            git(cwd, "config", "user.email", "test@example.test")
            path = cwd / "iroh/src/socket"
            path.mkdir(parents=True)
            (path / "a.rs").write_text("pub fn a() {}\n")
            git(cwd, "add", ".")
            git(cwd, "commit", "-qm", "base")
            base = git(cwd, "rev-parse", "HEAD")
            for branch, msg in [("first", "one"), ("second", "two")]:
                git(cwd, "checkout", "-q", "-b", branch, base)
                (path / "a.rs").write_text("pub fn a() { println!(\"ok\"); }\n")
                git(cwd, "add", ".")
                git(cwd, "commit", "-qm", msg)
            first = git(cwd, "rev-parse", "first")
            second = git(cwd, "rev-parse", "second")
            self.assertNotEqual(first, second)
            self.assertEqual(audit.patch_id(cwd / ".git", base, first),
                             audit.patch_id(cwd / ".git", base, second))
            paths = audit.changed_paths(cwd / ".git", base, first)
            self.assertEqual(paths, ["iroh/src/socket/a.rs"])
            self.assertEqual(audit.patch_id(cwd / ".git", base, first, paths),
                             audit.patch_id(cwd / ".git", base, second, paths))

    def test_shards_are_deterministic(self):
        sha = "a" * 40
        self.assertEqual(audit.shard_for(sha, 8), audit.shard_for(sha, 8))
        self.assertTrue(0 <= audit.shard_for(sha, 8) < 8)


if __name__ == "__main__":
    unittest.main()
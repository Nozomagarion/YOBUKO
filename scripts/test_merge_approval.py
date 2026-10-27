# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Run the real merge decision shell against a recording gh function, without GitHub writes."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class MergeApproval(unittest.TestCase):
    def run_queue(self, sender="Maintainer", label=True, changed_head=False):
        workflow = (ROOT / ".github/workflows/ready-to-merge.yml").read_text(encoding="utf-8")
        step = workflow.split("      - name: Approve the commit and turn on merge when ready", 1)[1]
        source = step.split("        run: |\n", 1)[1].split("\n  unlabel:", 1)[0]
        source = "\n".join(line[10:] for line in source.splitlines())
        bash = shutil.which("bash")
        if os.name == "nt":
            bash = str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe")
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "calls"
            stub = '''gh() {
  printf '%s\\n' "$*" >> "$CALLS"
  if [[ "$*" == *'/reviews'* && "$*" != *'--method POST'* ]]; then return 0; fi
  if [[ "$*" == *'--auto'* && "$CHANGED_HEAD" == true ]]; then return 1; fi
}
'''
            env = dict(os.environ, MAINTAINERS="Maintainer", SENDER=sender, AUTHOR="Contributor",
                       AUTHOR_TYPE="User", HEAD_REF="feature/panel", HAS_TEST_BUILD="false",
                       GITHUB_REPOSITORY="owner/repo", NUMBER="960", SHA="a" * 40,
                       LABEL="ready to merge", CALLS=log.as_posix(), CHANGED_HEAD=str(changed_head).lower(),
                       HELPERS=f"has_label() {{ return {0 if label else 1}; }}\nstop_merge() {{ gh stop; }}")
            result = subprocess.run([bash, "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", stub + source],
                                    env=env, capture_output=True, text=True)
            return result, log.read_text() if log.exists() else ""

    def test_maintainer_can_approve_without_ordering_a_build_or_repeating_review(self):
        result, calls = self.run_queue()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("commit_id=" + "a" * 40, calls)
        self.assertIn("--match-head-commit " + "a" * 40, calls)
        self.assertNotIn("--remove-label", calls)
        self.assertNotIn("test-build", calls)

    def test_non_maintainer_cannot_approve(self):
        _, calls = self.run_queue(sender="Contributor")
        self.assertIn("--remove-label", calls)
        self.assertNotIn("event=APPROVE", calls)
        self.assertNotIn("--auto", calls)

    def test_removed_label_does_not_queue(self):
        _, calls = self.run_queue(label=False)
        self.assertNotIn("--auto", calls)

    def test_new_head_is_not_queued_with_an_older_approval(self):
        result, calls = self.run_queue(changed_head=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--match-head-commit " + "a" * 40, calls)


if __name__ == "__main__":
    unittest.main()

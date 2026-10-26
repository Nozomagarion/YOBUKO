# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import json
from pathlib import Path
import tempfile
import unittest

import fork_readme


class ProgressTest(unittest.TestCase):
    def test_completed_claims_require_real_local_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            area = {"name": "Area", "color": "#ffffff", "milestones": [{"name": "A claim", "done": True}]}
            data = {"schema": 1, "areas": [area, area, area]}
            path = root / "fork-progress.json"
            path.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                fork_readme.progress(root)
            area["milestones"][0]["evidence"] = "proof.txt"
            path.write_text(json.dumps(data))
            (root / "proof.txt").write_text("Acceptance evidence")
            self.assertEqual(len(fork_readme.progress(root)), 3)

    def test_committed_presentation_matches_reviewed_criteria(self):
        fork_readme.generate(check=True)


if __name__ == "__main__":
    unittest.main()

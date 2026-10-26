# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Distribution source pins and complete-package regression checks (no network)."""
import json
from pathlib import Path
import tempfile
import unittest
import fork_package
import fork_sources


class ForkDistribution(unittest.TestCase):
    def test_vendored_source_tampering_is_detected(self):
        root = Path(__file__).resolve().parent.parent
        pins = json.loads((root / 'fork-sources.json').read_text())
        self.assertEqual(fork_sources.digests(root / fork_sources.VENDOR / 'src'), pins['rbxport']['files'])
        with tempfile.TemporaryDirectory() as folder:
            p = Path(folder) / 'lib.rs'
            p.write_text('original')
            before = fork_sources.digests(Path(folder))
            p.write_text('changed')
            self.assertNotEqual(before, fork_sources.digests(Path(folder)))
            p.unlink()
            p.symlink_to(root / 'crates/collaboration/src/lib.rs')
            with self.assertRaises(ValueError):
                fork_sources.digests(Path(folder))

    def test_a_package_has_all_binaries_licenses_and_exact_source_versions(self):
        root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as folder:
            package = Path(folder)
            with self.assertRaises(ValueError):
                fork_package.write(root, package, '0.0.1', 'a' * 40)
            for name in ['suhina', 'suhina-cli', 'suhina-plugin-host']:
                (package / name).write_bytes(b'fixture')
            fork_package.write(root, package, '0.0.1', 'a' * 40)
            marker = json.loads((package / 'DISTRIBUTION.json').read_text())
            self.assertEqual(marker['distribution'], 'suhina-multiplayer-dj')
            self.assertEqual(marker['source_commit'], 'a' * 40)
            self.assertEqual(marker['multiplayer_api'], 1)
            self.assertEqual(marker['dj_catalog'], 1)
            pins = json.loads((package / 'FORK-SOURCES.json').read_text())
            self.assertEqual(marker['rbxport_commit'], pins['rbxport']['commit'])
            self.assertTrue((package / 'RBXPORT-COPYING').is_file())

    def test_a_missing_source_commit_or_invalid_version_cannot_be_packaged(self):
        root = Path(__file__).resolve().parent.parent
        for version, commit in [('bad version', 'a' * 40), ('0.0.1', 'main'), ('0.0.1', '')]:
            with self.assertRaises(ValueError):
                fork_package.marker(root, version, commit)


if __name__ == '__main__':
    unittest.main()

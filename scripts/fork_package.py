#!/usr/bin/env python3
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Write the fork's distribution marker beside the three packaged executables.

Use before signing a Mac bundle. This does not publish or install anything.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess


def marker(root, version, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("a full source commit is required")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version):
        raise ValueError("a semantic version is required")
    pins = json.loads((root / "fork-sources.json").read_text())
    return {"schema": 1, "distribution": pins["distribution"], "version": version,
            "source_commit": commit, "multiplayer_api": 1, "dj_catalog": 1,
            "suhina_commit": pins["suhina"]["commit"], "rbxport_commit": pins["rbxport"]["commit"]}


def write(root, directory, version, commit):
    suffix = ".exe" if (directory / "suhina.exe").exists() else ""
    for name in ["suhina", "suhina-cli", "suhina-plugin-host"]:
        if not (directory / (name + suffix)).is_file():
            raise ValueError(f"missing packaged executable: {name}")
    for source in ["LICENSE", "COPYING", "VENDORED.md"]:
        (directory / ("RBXPORT-" + source)).write_bytes((root / "third_party/rbxport-analysis" / source).read_bytes())
    (directory / "DISTRIBUTION.json").write_text(json.dumps(marker(root, version, commit), indent=2) + "\n")
    (directory / "FORK-SOURCES.json").write_bytes((root / "fork-sources.json").read_bytes())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    write(root, args.directory, args.version, commit)

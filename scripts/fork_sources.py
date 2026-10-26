#!/usr/bin/env python3
# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Verify pinned sources, or prepare an isolated integration of both upstream projects.

prepare never pushes, publishes, resets main or modifies the caller's checkout.
Run the printed validation commands before accepting the candidate branch.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
from datetime import datetime, timezone

SUHINA = "https://github.com/suhina-daw/suhina-daw.git"
RBXPORT = "https://github.com/chrisle/rbxport.git"
VENDOR = Path("third_party/rbxport-analysis")


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def digests(folder):
    files = sorted(folder.rglob("*.rs"))
    if not files or any(p.is_symlink() for p in files):
        raise ValueError("analysis sources must be regular Rust files")
    return {p.relative_to(folder).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}


def refresh(root, source, suhina_commit):
    manifest = tomllib.loads((source / "crates/rbl-analysis/Cargo.toml").read_text())
    dependencies = manifest["dependencies"]
    if dependencies != {"thiserror": {"workspace": True}, "tracing": {"workspace": True}, "realfft": "3"}:
        raise ValueError("rbxport analysis dependencies changed: review the adapter before integrating")
    version = tomllib.loads((source / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    target = root / VENDOR
    source_tree = source / "crates/rbl-analysis/src"
    hashes = digests(source_tree)
    shutil.rmtree(target / "src")
    shutil.copytree(source_tree, target / "src")
    text = (target / "Cargo.toml").read_text()
    import re
    text = re.sub(r'^version = "[^"]+"', f'version = "{version}"', text, count=1, flags=re.M)
    (target / "Cargo.toml").write_text(text)
    shutil.copyfile(source / "LICENSE", target / "LICENSE")
    shutil.copyfile(source / "crates/rbl-deck/vendor/rubberband/COPYING", target / "COPYING")
    commit = git(source, "rev-parse", "HEAD")
    pins = json.loads((root / "fork-sources.json").read_text())
    pins["suhina"]["commit"] = suhina_commit
    pins["rbxport"].update(commit=commit, version=version, files=hashes)
    (root / "fork-sources.json").write_text(json.dumps(pins, indent=2) + "\n")
    (target / "VENDORED.md").write_text(
        f"# rbxport analysis\n\nSource: https://github.com/chrisle/rbxport/tree/{commit}/crates/rbl-analysis\n\n"
        "Copyright (c) 2026 Chris Le. GPL-2.0-or-later; see LICENSE and COPYING.\n"
        "The src/ tree is copied byte for byte. Cargo.toml is adapted to build without\n"
        "Tauri or private-library development dependencies. Only offline analysis is integrated.\n"
        "Refresh through scripts/fork_sources.py and validate before shipping.\n")


def check(root):
    pins = json.loads((root / "fork-sources.json").read_text())
    if pins["suhina"]["repository"] != SUHINA or pins["rbxport"]["repository"] != RBXPORT:
        raise ValueError("unexpected source repository")
    git(root, "merge-base", "--is-ancestor", pins["suhina"]["commit"], "HEAD")
    if digests(root / VENDOR / "src") != pins["rbxport"]["files"]:
        raise ValueError("vendored rbxport analysis differs from its pinned source hashes")
    if not (root / "crates/collaboration/src/lib.rs").is_file():
        raise ValueError("the multiplayer module is absent")
    print(f"Sources verified: Suhina {pins['suhina']['commit'][:12]}, rbxport {pins['rbxport']['commit'][:12]}")


def prepare(root, base, directory):
    if directory.exists():
        raise ValueError("the integration directory must not exist")
    # Fetch into dedicated refs; neither main nor the feature branch is moved.
    git(root, "fetch", SUHINA, "main:refs/fork-sources/suhina")
    commit = git(root, "rev-parse", "refs/fork-sources/suhina")
    branch = "integration/multiplayer-" + datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    git(root, "worktree", "add", "-b", branch, str(directory), base)
    # Conflicts are deliberately left in this isolated candidate for a semantic resolution.
    git(directory, "merge", "--no-edit", commit)
    with tempfile.TemporaryDirectory(prefix="suhina-rbxport-") as temp:
        source = Path(temp)
        git(source, "init", "--quiet")
        git(source, "fetch", "--depth", "1", RBXPORT, "main")
        git(source, "checkout", "--detach", "FETCH_HEAD")
        refresh(directory, source, commit)
    check(directory)
    print(f"Candidate: {branch}\nDirectory: {directory}\nNo push or release was performed.")
    print("Run cargo check to refresh Cargo.lock if required; then cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings,")
    print("cargo test --workspace, cargo deny check, python3 scripts/docs.py check, and the fork CI before accepting.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "prepare"])
    parser.add_argument("--base", default="feature/multiplayer")
    parser.add_argument("--directory", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    if args.command == "check":
        check(root)
    elif args.directory is None:
        parser.error("prepare requires --directory pointing to a new worktree")
    else:
        prepare(root, args.base, args.directory.resolve())


if __name__ == "__main__":
    main()

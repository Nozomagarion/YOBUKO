// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A fork build only accepts complete packages published by its own distribution.

use crate::Build;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

pub const ID: &str = "suhina-multiplayer-dj";
pub const REPOSITORY: &str = "Nozomagarion/suhina-daw";
pub const TAG: &str = "multiplayer";
pub const MARKER: &str = "DISTRIBUTION.json";
pub const SOURCES: &str = include_str!("../../../fork-sources.json");

pub fn validate_build(build: &Build) -> Result<(), String> {
    let release = format!("https://github.com/{REPOSITORY}/releases/download/{TAG}/");
    let artifact = format!("https://api.github.com/repos/{REPOSITORY}/actions/artifacts/");
    let valid_url = if build.needs_sign_in {
        build.url.strip_prefix(&artifact).is_some_and(|s| {
            s.strip_suffix("/zip").is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        })
    } else {
        build.url.strip_prefix(&release).is_some_and(|name| {
            name.starts_with("suhina-multiplayer-")
                && !name.contains(['/', '?', '#', '%'])
                && (name.ends_with(".tar.gz") || name.ends_with(".zip"))
        })
    };
    if !valid_url {
        return Err("this build is not from the multiplayer distribution; not installed".into());
    }
    if !build.sha256.as_ref().is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err("the multiplayer package needs a SHA-256 checksum; not installed".into());
    }
    if semver::Version::parse(&build.version).is_err() || !commit(&build.commit) {
        return Err("the multiplayer build needs a version and full source commit; not installed".into());
    }
    Ok(())
}

fn commit(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The marker sits beside the executable, including inside Contents/MacOS in a Mac bundle.
/// Validate before replacing any running files. This is a distribution guard, not a signature.
pub fn validate_package(program: &Path, build: &Build) -> Result<(), String> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    for name in ["suhina", "suhina-cli", "suhina-plugin-host"] {
        if !program.join(format!("{name}{suffix}")).is_file() {
            return Err(format!("the combined package is missing {name}; not installed"));
        }
    }
    let file = std::fs::File::open(program.join(MARKER))
        .map_err(|_| "the download has no multiplayer distribution manifest; not installed")?;
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("the distribution manifest is too large".into());
    }
    let marker: Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid distribution manifest: {e}"))?;
    if marker["schema"] != 1
        || marker["distribution"] != ID
        || marker["version"] != build.version
        || marker["source_commit"] != build.commit
        || marker["multiplayer_api"] != 1
        || marker["dj_catalog"] != 1
        || !marker["suhina_commit"].as_str().is_some_and(commit)
        || !marker["rbxport_commit"].as_str().is_some_and(commit)
    {
        return Err(
            "the package does not preserve this distribution's multiplayer and DJ contracts; not installed".into()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;

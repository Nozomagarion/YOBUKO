// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn build() -> Build {
    Build {
        version: "0.0.1-nightly.20261026120000+abcdef0".into(),
        url: format!(
            "https://github.com/{REPOSITORY}/releases/download/{TAG}/suhina-multiplayer-macos-universal.tar.gz"
        ),
        sha256: Some("a".repeat(64)),
        commit: "abcdef0".to_string() + &"a".repeat(33),
        ..Build::default()
    }
}

#[test]
fn official_binaries_other_repositories_and_missing_checksums_cannot_replace_the_fork() {
    let good = build();
    assert!(validate_build(&good).is_ok());
    for url in [
        good.url.replace(REPOSITORY, "suhina-daw/suhina-daw"),
        good.url.replace("github.com/", "github.com.attacker.invalid/"),
        good.url.replace("suhina-multiplayer-", "suhina-nightly-"),
        format!("{}?other", good.url),
    ] {
        assert!(validate_build(&Build { url, ..good.clone() }).is_err());
    }
    assert!(validate_build(&Build { sha256: None, ..good.clone() }).is_err());
    assert!(validate_build(&Build { commit: "feature/multiplayer".into(), ..good }).is_err());
}

#[test]
fn only_a_matching_complete_manifest_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let build = build();
    assert!(validate_package(dir.path(), &build).is_err());
    for name in ["suhina", "suhina-cli", "suhina-plugin-host"] {
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        std::fs::write(dir.path().join(format!("{name}{suffix}")), b"fixture").unwrap();
    }
    let good = serde_json::json!({"schema":1,"distribution":ID,"version":build.version,
        "source_commit":build.commit,"multiplayer_api":1,"dj_catalog":1,
        "suhina_commit":"b".repeat(40),"rbxport_commit":"c".repeat(40)});
    let path = dir.path().join(MARKER);
    std::fs::write(&path, serde_json::to_vec(&good).unwrap()).unwrap();
    assert!(validate_package(dir.path(), &build).is_ok());
    for (key, value) in [
        ("distribution", Value::from("suhina-official")),
        ("multiplayer_api", Value::from(0)),
        ("dj_catalog", Value::from(2)),
        ("source_commit", Value::from("d".repeat(40))),
        ("version", Value::from("0.2.0")),
        ("rbxport_commit", Value::from("main")),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(validate_package(dir.path(), &build).is_err(), "{key}");
    }
}

#[test]
fn fork_source_pins_are_embedded_in_the_updater() {
    let sources: Value = serde_json::from_str(SOURCES).unwrap();
    assert_eq!(sources["distribution"], ID);
    assert!(sources["suhina"]["commit"].as_str().is_some_and(commit));
    assert!(sources["rbxport"]["commit"].as_str().is_some_and(commit));
    assert!(!crate::github::NIGHTLY_URL.contains("repos/suhina-daw/"));
    assert!(!crate::github::ARTIFACTS_URL.contains("repos/suhina-daw/"));
}

#[test]
fn a_mac_bundle_requires_metadata_in_resources_and_keeps_the_same_contract_guard() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("Suhina.app/Contents/MacOS");
    let resources = dir.path().join("Suhina.app/Contents/Resources");
    std::fs::create_dir_all(&program).unwrap();
    std::fs::create_dir_all(&resources).unwrap();
    let build = build();
    for name in ["suhina", "suhina-cli", "suhina-plugin-host"] {
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        std::fs::write(program.join(format!("{name}{suffix}")), b"fixture").unwrap();
    }
    let good = serde_json::json!({"schema":1,"distribution":ID,"version":build.version,
        "source_commit":build.commit,"multiplayer_api":1,"dj_catalog":1,
        "suhina_commit":"b".repeat(40),"rbxport_commit":"c".repeat(40)});
    std::fs::write(program.join(MARKER), serde_json::to_vec(&good).unwrap()).unwrap();
    assert!(
        validate_package(&program, &build).is_err(),
        "metadata in the executable folder requires nonportable detached signatures"
    );
    std::fs::remove_file(program.join(MARKER)).unwrap();
    std::fs::write(resources.join(MARKER), serde_json::to_vec(&good).unwrap()).unwrap();
    assert!(validate_package(&program, &build).is_ok());
    let mut bad = good;
    bad["multiplayer_api"] = Value::from(0);
    std::fs::write(resources.join(MARKER), serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(validate_package(&program, &build).is_err());
}

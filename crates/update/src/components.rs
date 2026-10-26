// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only GitHub source checks. Only the combined distribution is installable.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub name: &'static str,
    pub installed: String,
    pub latest: Result<String, String>,
    pub page: &'static str,
    pub fork: bool,
}

impl Component {
    pub fn status(&self) -> &'static str {
        match &self.latest {
            Err(_) => "Unavailable",
            Ok(_) if !revision(&self.installed) => "Local build",
            Ok(latest) if latest.starts_with(&self.installed) => "Up to date",
            Ok(_) if self.fork => "New development commit",
            Ok(_) => "Integration needed",
        }
    }
}

fn revision(text: &str) -> bool {
    (7..=40).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A branch response contains a small commit summary, rather than its whole diff.
pub fn parse_branch(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > 65536 {
        return Err("GitHub's branch answer is too large".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "GitHub's branch answer is not JSON")?;
    value["commit"]["sha"]
        .as_str()
        .filter(|s| s.len() == 40 && revision(s))
        .map(str::to_owned)
        .ok_or_else(|| "GitHub's branch answer has no full source revision".into())
}

pub fn check(fork_build: &str) -> Vec<Component> {
    let pins: Value = serde_json::from_str(super::distribution::SOURCES).unwrap_or(Value::Null);
    [
        (
            "Suhina",
            pins["suhina"]["commit"].as_str().unwrap_or("unknown"),
            "https://api.github.com/repos/suhina-daw/suhina-daw/branches/main",
            "https://github.com/suhina-daw/suhina-daw",
            false,
        ),
        (
            "rbxport",
            pins["rbxport"]["commit"].as_str().unwrap_or("unknown"),
            "https://api.github.com/repos/chrisle/rbxport/branches/main",
            "https://github.com/chrisle/rbxport",
            false,
        ),
        (
            "Multiplayer + DJ",
            fork_build,
            "https://api.github.com/repos/Nozomagarion/suhina-daw/branches/feature%2Fmultiplayer",
            "https://github.com/Nozomagarion/suhina-daw/tree/feature/multiplayer",
            true,
        ),
    ]
    .into_iter()
    .map(|(name, installed, url, page, fork)| Component {
        name,
        installed: installed.into(),
        latest: super::net::get(url, None).and_then(|b| parse_branch(&b)),
        page,
        fork,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branch_answers_need_a_full_revision_and_refuse_errors_and_oversized_input() {
        let sha = "a".repeat(40);
        assert_eq!(parse_branch(serde_json::json!({"commit":{"sha":sha}}).to_string().as_bytes()), Ok(sha));
        for bad in [b"not json".as_slice(), br#"{"message":"Not Found"}"#, br#"{"commit":{"sha":"main"}}"#] {
            assert!(parse_branch(bad).is_err());
        }
        assert!(parse_branch(&vec![b' '; 65537]).is_err());
    }
    #[test]
    fn upstream_changes_need_integration_and_fork_commits_are_not_releases() {
        let mut c =
            Component { name: "test", installed: "a".repeat(40), latest: Ok("b".repeat(40)), page: "", fork: false };
        assert_eq!(c.status(), "Integration needed");
        c.fork = true;
        assert_eq!(c.status(), "New development commit");
        c.latest = Ok("a".repeat(40));
        assert_eq!(c.status(), "Up to date");
        c.installed = "unknown".into();
        assert_eq!(c.status(), "Local build");
        c.latest = Err("offline".into());
        assert_eq!(c.status(), "Unavailable");
    }
}

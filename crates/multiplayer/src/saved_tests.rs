// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn info(main: bool) -> SessionInfo {
    SessionInfo { main, ..SessionInfo::create("Sunday jam", "Nozo").unwrap() }
}

fn keep(root: &Path, info: &SessionInfo, mine: bool, at: u64) -> PathBuf {
    let folder = place(root, info).unwrap();
    write(&folder, info, mine, at).unwrap();
    std::fs::write(folder.join(PROJECT_FILE), b"project").unwrap();
    folder
}

#[test]
fn saved_sessions_are_listed_newest_first_with_who_keeps_the_main_version() {
    let root = tempfile::tempdir().unwrap();
    let older = info(true);
    let newer = SessionInfo { owner: "Friend".into(), ..info(true) };
    keep(root.path(), &older, true, 10);
    keep(root.path(), &newer, false, 20);
    let listed = list(root.path());
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].info, newer);
    assert!(!listed[0].mine);
    assert_eq!(listed[1].info, older);
    assert!(listed[1].mine);
    assert_eq!(read(&listed[1].folder).unwrap(), listed[1]);
}

#[test]
fn a_backup_copy_never_overwrites_the_main_version_kept_here() {
    let root = tempfile::tempdir().unwrap();
    let main = info(true);
    let own = keep(root.path(), &main, true, 10);
    // A friend hosts their backup copy while the creator is away; the creator joins it.
    let copy = SessionInfo { main: false, ..main.clone() };
    let elsewhere = keep(root.path(), &copy, false, 20);
    assert_ne!(own, elsewhere);
    assert!(read(&own).unwrap().mine);
    // On a friend's computer, the backup copy is their own folder.
    let friend = tempfile::tempdir().unwrap();
    assert_eq!(place(friend.path(), &copy).unwrap(), folder(friend.path()).join(&copy.id));
}

#[test]
fn a_session_without_its_project_file_or_with_a_bad_file_is_not_listed() {
    let root = tempfile::tempdir().unwrap();
    let info = info(true);
    write(&place(root.path(), &info).unwrap(), &info, true, 1).unwrap();
    std::fs::create_dir_all(folder(root.path()).join("junk")).unwrap();
    std::fs::write(folder(root.path()).join("junk").join(META_FILE), b"{not json").unwrap();
    assert!(list(root.path()).is_empty());
}

#[test]
fn my_name_is_kept_and_checked() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(my_name(root.path()), None);
    assert!(set_my_name(root.path(), "  ").is_err());
    assert!(set_my_name(root.path(), &"x".repeat(MAX_SESSION_TEXT + 1)).is_err());
    set_my_name(root.path(), " Nozo ").unwrap();
    assert_eq!(my_name(root.path()).as_deref(), Some("Nozo"));
}

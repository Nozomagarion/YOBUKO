// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The tables of Mixxx 2.5's library (schema 39) that the bridge touches, with the
/// columns it reads or writes.
const SCHEMA: &str = "
CREATE TABLE track_locations (id INTEGER PRIMARY KEY AUTOINCREMENT, location varchar(512) UNIQUE,
  filename varchar(512), directory varchar(512), filesize INTEGER, fs_deleted INTEGER, needs_verification INTEGER);
CREATE TABLE library (id INTEGER PRIMARY KEY AUTOINCREMENT, artist varchar(64), title varchar(64),
  album varchar(64), year varchar(16), genre varchar(64), tracknumber varchar(3),
  location INTEGER REFERENCES track_locations(location), comment varchar(256), url varchar(256),
  duration FLOAT, bitrate INTEGER, samplerate INTEGER, cuepoint INTEGER, bpm FLOAT, wavesummaryhex BLOB,
  channels INTEGER, datetime_added DEFAULT CURRENT_TIMESTAMP, mixxx_deleted INTEGER, played INTEGER,
  header_parsed INTEGER DEFAULT 0, filetype varchar(8) DEFAULT '?', replaygain FLOAT DEFAULT 0,
  timesplayed INTEGER DEFAULT 0, rating INTEGER DEFAULT 0, key varchar(8) DEFAULT '');
CREATE TABLE Playlists (id INTEGER PRIMARY KEY, name varchar(48), position INTEGER,
  hidden INTEGER DEFAULT 0 NOT NULL, date_created datetime, date_modified datetime, locked INTEGER DEFAULT 0);
CREATE TABLE PlaylistTracks (id INTEGER PRIMARY KEY, playlist_id INTEGER REFERENCES Playlists(id),
  track_id INTEGER REFERENCES library(id), position INTEGER, pl_datetime_added);
CREATE TABLE cues (id INTEGER PRIMARY KEY AUTOINCREMENT, track_id INTEGER NOT NULL REFERENCES library(id),
  type INTEGER DEFAULT 0 NOT NULL, position INTEGER DEFAULT -1 NOT NULL, length INTEGER DEFAULT 0 NOT NULL,
  hotcue INTEGER DEFAULT -1 NOT NULL, label TEXT DEFAULT '' NOT NULL, color INTEGER DEFAULT 4294901760 NOT NULL);
CREATE TABLE crates (id INTEGER PRIMARY KEY AUTOINCREMENT, name varchar(48) UNIQUE NOT NULL,
  count INTEGER DEFAULT 0, show INTEGER DEFAULT 1, locked INTEGER DEFAULT 0, autodj_source INTEGER DEFAULT 0);
CREATE TABLE crate_tracks (crate_id INTEGER NOT NULL REFERENCES crates(id),
  track_id INTEGER NOT NULL REFERENCES library(id), UNIQUE (crate_id, track_id));
";

struct Fixture {
    dir: tempfile::TempDir,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(DATABASE);
        Connection::open(&db).unwrap().execute_batch(SCHEMA).unwrap();
        Self { dir, db }
    }

    /// An audio file that exists, by name.
    fn file(&self, name: &str) -> String {
        let path = self.dir.path().join(name);
        std::fs::write(&path, b"ID3").unwrap();
        path.to_string_lossy().into_owned()
    }

    /// A track Mixxx already has.
    fn mixxx_track(&self, path: &str) -> i64 {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO track_locations (location, filename, directory, filesize, fs_deleted, needs_verification)
             VALUES (?1, 'x', 'd', 1, 0, 0)",
            [mixxx_location(path)],
        )
        .unwrap();
        let location = conn.last_insert_rowid();
        conn.execute("INSERT INTO library (title, location, mixxx_deleted) VALUES ('Mine', ?1, 0)", [location])
            .unwrap();
        conn.last_insert_rowid()
    }

    fn conn(&self) -> Connection {
        let conn = Connection::open(&self.db).unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn
    }
}

fn track(path: &str, cues: Vec<MirrorCue>) -> MirrorTrack {
    MirrorTrack {
        path: path.into(),
        title: "Song".into(),
        artist: "Artist".into(),
        bpm: 124.0,
        seconds: 240,
        sample_rate: 44_100,
        rating: 4,
        key: "8A".into(),
        hot_cues: cues,
        ..Default::default()
    }
}

#[test]
fn rbxport_tracks_reach_mixxx_with_their_hot_cues_and_loops() {
    let f = Fixture::new();
    let a = f.file("a.mp3");
    let mirror = Mirror {
        format: 1,
        tracks: vec![track(
            &a,
            vec![MirrorCue { slot: 0, ms: 1000, loop_end_ms: 0 }, MirrorCue { slot: 2, ms: 2000, loop_end_ms: 4000 }],
        )],
        playlists: vec![],
    };
    let done = import_into_mixxx(&f.db, &mirror).unwrap();
    assert_eq!(done, Imported { tracks: 1, cues: 2, playlists: 0 });
    let conn = f.conn();
    let (title, bpm, rating, parsed): (String, f64, i64, i64) = conn
        .query_row("SELECT title, bpm, rating, header_parsed FROM library", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((title.as_str(), bpm, rating, parsed), ("Song", 124.0, 4, 0));
    let verify: i64 = conn.query_row("SELECT needs_verification FROM track_locations", [], |r| r.get(0)).unwrap();
    assert_eq!(verify, 1, "Mixxx re-reads the file's own tags");
    let cues: Vec<(i64, f64, f64, i64)> = conn
        .prepare("SELECT type, position, length, hotcue FROM cues ORDER BY hotcue")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // One second at 44.1 kHz is 88 200 samples of both channels.
    assert_eq!(cues, vec![(HOT_CUE, 88_200.0, 0.0, 0), (LOOP, 176_400.0, 176_400.0, 2)]);

    // A second sync adds nothing twice.
    assert_eq!(import_into_mixxx(&f.db, &mirror).unwrap().tracks, 0);
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM library", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn a_track_mixxx_has_keeps_its_own_cues_and_a_missing_file_is_left_out() {
    let f = Fixture::new();
    let a = f.file("a.mp3");
    let mine = f.mixxx_track(&a);
    let gone = f.dir.path().join("gone.mp3").to_string_lossy().into_owned();
    let mirror = Mirror {
        format: 1,
        tracks: vec![track(&a, vec![MirrorCue { slot: 1, ms: 500, loop_end_ms: 0 }]), track(&gone, vec![])],
        playlists: vec![MirrorPlaylist { folders: vec![], name: "Set".into(), tracks: vec![0, 1] }],
    };
    let done = import_into_mixxx(&f.db, &mirror).unwrap();
    assert_eq!(done, Imported { tracks: 0, cues: 0, playlists: 1 });
    let conn = f.conn();
    let members: Vec<i64> = conn
        .prepare("SELECT track_id FROM PlaylistTracks ORDER BY position")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(members, vec![mine]);
}

#[test]
fn rbxport_playlists_are_refilled_and_removed_with_rbxport_and_mixxx_ones_stay_out() {
    let f = Fixture::new();
    let a = f.file("a.mp3");
    let b = f.file("b.mp3");
    let mut mirror = Mirror {
        format: 1,
        tracks: vec![track(&a, vec![]), track(&b, vec![])],
        playlists: vec![
            MirrorPlaylist { folders: vec!["Club".into()], name: "Peak".into(), tracks: vec![0, 1] },
            MirrorPlaylist { folders: vec![FROM_MIXXX.into()], name: "Back".into(), tracks: vec![0] },
        ],
    };
    import_into_mixxx(&f.db, &mirror).unwrap();
    let names = |conn: &Connection| -> Vec<String> {
        conn.prepare("SELECT name FROM Playlists ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let conn = f.conn();
    assert_eq!(names(&conn), vec!["rbxport · Club / Peak".to_owned()]);

    mirror.playlists[0].tracks = vec![1];
    import_into_mixxx(&f.db, &mirror).unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM PlaylistTracks", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);

    // The DJ's own Mixxx playlist is never touched.
    conn.execute("INSERT INTO Playlists (name, position, hidden) VALUES ('Mine', 9, 0)", []).unwrap();
    mirror.playlists.clear();
    import_into_mixxx(&f.db, &mirror).unwrap();
    assert_eq!(names(&conn), vec!["Mine".to_owned()]);
}

#[test]
fn mixxx_tracks_playlists_and_crates_go_to_rbxport_once() {
    let f = Fixture::new();
    let a = f.file("a.mp3");
    let b = f.file("b.mp3");
    let ta = f.mixxx_track(&a);
    let tb = f.mixxx_track(&b);
    let conn = f.conn();
    conn.execute("INSERT INTO Playlists (id, name, position, hidden) VALUES (1, 'Friday', 1, 0)", []).unwrap();
    conn.execute("INSERT INTO Playlists (id, name, position, hidden) VALUES (2, 'Auto DJ', 2, 1)", []).unwrap();
    conn.execute("INSERT INTO Playlists (id, name, position, hidden) VALUES (3, 'rbxport · Peak', 3, 0)", []).unwrap();
    for (playlist, t, pos) in [(1, tb, 1), (1, ta, 2), (2, ta, 1), (3, ta, 1)] {
        conn.execute(
            "INSERT INTO PlaylistTracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist, t, pos],
        )
        .unwrap();
    }
    conn.execute("INSERT INTO crates (id, name, show) VALUES (1, 'House', 1)", []).unwrap();
    conn.execute("INSERT INTO crate_tracks (crate_id, track_id) VALUES (1, ?1)", [ta]).unwrap();

    let mixxx = read_mixxx(&f.db).unwrap();
    assert_eq!(mixxx.tracks.len(), 2);
    let names: Vec<_> = mixxx.lists.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["Friday", "House"]);

    let first = to_rbxport(&mixxx, None);
    assert_eq!(first.tracks.len(), 2);
    assert_eq!(first.playlists.len(), 2);
    assert_eq!(first.playlists[0].tracks, vec![mixxx_location(&b), mixxx_location(&a)]);
    assert_eq!(first.playlists[1].name, format!("{CRATE_PREFIX}House"));

    // Once rbxport has them, nothing is asked again.
    let mirror = Mirror {
        format: 1,
        tracks: vec![track(&a, vec![]), track(&b, vec![])],
        playlists: vec![
            MirrorPlaylist { folders: vec![FROM_MIXXX.into()], name: "Friday".into(), tracks: vec![1, 0] },
            MirrorPlaylist { folders: vec![FROM_MIXXX.into()], name: format!("{CRATE_PREFIX}House"), tracks: vec![0] },
        ],
    };
    assert!(to_rbxport(&mixxx, Some(&mirror)).is_empty());
}

#[test]
fn requests_replace_unread_ones_and_are_written_whole() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("sync-1.paths"), "/old\n").unwrap();
    std::fs::write(inbox.join("123.paths"), "/sent-by-hand\n").unwrap();
    let requests = ToRbxport {
        tracks: vec!["/m/a.mp3".into()],
        playlists: vec![PlaylistRequest {
            folder: FROM_MIXXX.into(),
            name: "Friday".into(),
            tracks: vec!["/m/a.mp3".into()],
        }],
    };
    assert_eq!(write_requests(&inbox, &requests).unwrap(), 2);
    let mut names: Vec<String> =
        std::fs::read_dir(&inbox).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names.len(), 3);
    assert_eq!(names[0], "123.paths");
    assert!(names[1].starts_with(QUIET) && names[1].ends_with(".paths"));
    assert!(names[2].starts_with(QUIET) && names[2].ends_with(".playlist"));
    let playlist: PlaylistRequest = serde_json::from_slice(&std::fs::read(inbox.join(&names[2])).unwrap()).unwrap();
    assert_eq!(playlist, requests.playlists[0]);
}

#[test]
fn an_unopened_mixxx_is_said_plainly() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_mixxx(&dir.path().join(DATABASE)).unwrap_err().contains("no library yet"));
    let empty = dir.path().join("empty.sqlite");
    Connection::open(&empty).unwrap().execute_batch("CREATE TABLE t (x)").unwrap();
    assert!(import_into_mixxx(&empty, &Mirror::default()).unwrap_err().contains("open Mixxx once"));
}

#[test]
fn the_flx4_is_enabled_with_the_mapping_for_the_mixxx_version() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("Mixxx");
    configure(&profile, Mapping::BuiltIn).unwrap();
    let settings = Settings::parse(&std::fs::read_to_string(profile.join(CONFIG)).unwrap());
    for name in FLX4_NAMES {
        assert_eq!(settings.get("[ControllerPreset]", name), Some("Pioneer-DDJ-FLX4.midi.xml"));
        assert_eq!(settings.get("[Controller]", name), Some("1"));
    }
    assert_eq!(settings.get("[Config]", "Version"), None, "Mixxx still runs its first-start setup");

    // Mixxx 2.6: the community mapping, installed with its Extended preset.
    configure(&profile, Mapping::for_version(Some((2, 6)))).unwrap();
    let settings = Settings::parse(&std::fs::read_to_string(profile.join(CONFIG)).unwrap());
    assert_eq!(settings.get("[ControllerPreset]", "DDJ-FLX4"), Some(EXTENDED));
    let extended = std::fs::read_to_string(profile.join("controllers").join(EXTENDED)).unwrap();
    assert!(extended.contains("<name>Pioneer DDJ-FLX4 (Extended)</name>"));
    let basic_script = extended.find("Pioneer-DDJ-FLX4-2.0.script.js").unwrap();
    let extended_script = extended.find("Pioneer-DDJ-FLX4-extended-scripts.js").unwrap();
    assert!(basic_script < extended_script, "Basic loads first");
    assert!(profile.join("effects/chains/14_PHASER_DJ.xml").is_file());
}

#[test]
fn a_mapping_the_dj_chose_in_mixxx_is_kept_and_other_settings_survive() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().to_path_buf();
    std::fs::write(
        profile.join(CONFIG),
        "[Config]\nVersion 2.5.6\n\n[ControllerPreset]\nDDJ-FLX4 /home/dj/my-own.midi.xml\n\n[Controller]\nDDJ-FLX4 0\n",
    )
    .unwrap();
    configure(&profile, Mapping::BuiltIn).unwrap();
    let settings = Settings::parse(&std::fs::read_to_string(profile.join(CONFIG)).unwrap());
    assert_eq!(settings.get("[Config]", "Version"), Some("2.5.6"));
    assert_eq!(settings.get("[ControllerPreset]", "DDJ-FLX4"), Some("/home/dj/my-own.midi.xml"));
    assert_eq!(settings.get("[Controller]", "DDJ-FLX4"), Some("0"), "switched off by the DJ: stays off");
    assert_eq!(settings.get("[ControllerPreset]", "DDJ-FLX4_MIDI_1"), Some("Pioneer-DDJ-FLX4.midi.xml"));
}

#[test]
fn versions_read_from_a_note_or_an_info_plist() {
    assert_eq!(parse_version("2.5.6\n"), Some((2, 5)));
    assert_eq!(parse_version("Mixxx 2.6-beta-464"), Some((2, 6)));
    assert_eq!(parse_version("nothing"), None);
    let plist = "<dict><key>CFBundleName</key><string>Mixxx</string>\
                 <key>CFBundleShortVersionString</key><string>2.6.0</string></dict>";
    assert_eq!(plist_version(plist), Some((2, 6)));
    assert_eq!(Mapping::for_version(None), Mapping::BuiltIn);
    assert_eq!(Mapping::for_version(Some((2, 4))), Mapping::BuiltIn);
    assert_eq!(Mapping::for_version(Some((2, 5))), Mapping::CommunityBasic);
    assert_eq!(Mapping::for_version(Some((2, 6))), Mapping::Community);
}

#[test]
fn mixxx_2_5_gets_the_community_mappings_basic_preset_and_its_effect_chains() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("Mixxx");
    configure(&profile, Mapping::for_version(Some((2, 5)))).unwrap();
    let settings = Settings::parse(&std::fs::read_to_string(profile.join(CONFIG)).unwrap());
    for name in FLX4_NAMES {
        assert_eq!(settings.get("[ControllerPreset]", name), Some(BASIC));
    }
    let basic = std::fs::read_to_string(profile.join("controllers").join(BASIC)).unwrap();
    assert!(basic.contains("<name>Pioneer DDJ-FLX4 (Basic)</name>"));
    assert!(!basic.contains("Stem"), "nothing Mixxx 2.5 lacks");
    assert!(profile.join("controllers/Pioneer-DDJ-FLX4-2.0.script.js").is_file());
    assert!(profile.join("effects/chains/01_ECHO_1_4.xml").is_file());
}

#[test]
fn paths_compare_across_the_two_apps() {
    assert_eq!(same_file_key("C:\\Music\\A.mp3"), same_file_key("C:/Music/A.mp3"));
    let playlist = MirrorPlaylist { folders: vec!["Club".into(), "2026".into()], name: "Peak".into(), tracks: vec![] };
    assert_eq!(mixxx_playlist_name(&playlist), "rbxport · Club / 2026 / Peak");
}

#[test]
fn a_rendered_track_joins_mixxx_without_touching_its_playlists() {
    let f = Fixture::new();
    let a = f.file("a.mp3");
    let b = f.file("render.wav");
    let mirror = Mirror {
        format: 1,
        tracks: vec![track(&a, vec![])],
        playlists: vec![MirrorPlaylist { folders: vec![], name: "Set".into(), tracks: vec![0] }],
    };
    import_into_mixxx(&f.db, &mirror).unwrap();
    let done = add_to_mixxx(&f.db, &[MirrorTrack { path: b, ..Default::default() }]).unwrap();
    assert_eq!(done.tracks, 1);
    let conn = f.conn();
    let playlists: i64 = conn.query_row("SELECT COUNT(*) FROM Playlists", [], |r| r.get(0)).unwrap();
    assert_eq!(playlists, 1);
    let title: String = conn.query_row("SELECT title FROM library ORDER BY id DESC", [], |r| r.get(0)).unwrap();
    assert_eq!(title, "render", "named after its file until Mixxx reads its tags");
}

#[test]
fn a_new_profile_gets_its_music_folder_so_mixxx_asks_for_none() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("Mixxx");
    let music = dir.path().join("My Music");
    assert!(seed_music_folder(&profile, &music).unwrap());
    let settings = Settings::parse(&std::fs::read_to_string(profile.join(CONFIG)).unwrap());
    assert_eq!(settings.get("[Playlist]", "Directory"), Some(music.to_string_lossy().as_ref()));
    assert!(!seed_music_folder(&profile, Path::new("/elsewhere")).unwrap(), "set once");
    let other = dir.path().join("Ran before");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join(DATABASE), b"").unwrap();
    assert!(!seed_music_folder(&other, &music).unwrap(), "Mixxx has its library already");
}

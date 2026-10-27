// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;
use suhina_model::{DeviceKind, InputChannels, MidiPort, Monitor, Project, Rgb};
use suhina_session::Session;

#[test]
fn shared_audio_and_midi_tracks_have_deterministic_ids_inputs_off_and_default_sound() {
    let mut a = Session::new(Project::default());
    a.execute("scene.insert", &json!({"name":"Verse"})).unwrap();
    let mut b = Session::new(a.project().clone());
    a.set_track_type_colours(Rgb(1, 2, 3), Rgb(4, 5, 6));
    b.set_track_type_colours(Rgb(7, 8, 9), Rgb(10, 11, 12));
    for kind in ["audio", "midi"] {
        let before = a.project().clone();
        let params = json!({"kind":kind,"name":format!("New {kind}")});
        let value = a.execute("track.createShared", &params).unwrap();
        assert_eq!(value, b.execute("track.createShared", &params).unwrap());
        assert_eq!(a.project(), b.project());
        let p = a.project();
        let track = p.tracks.last().unwrap();
        assert_eq!(track.midi_input.port, MidiPort::Nothing);
        assert_eq!(track.midi_input.monitor, Monitor::Off);
        assert_eq!(track.audio_input.channels, InputChannels::None);
        assert_eq!(track.audio_input.monitor, Monitor::Off);
        assert_eq!(p.launcher.columns.last().unwrap().slots.len(), 1);
        assert!(p.launcher.columns.last().unwrap().slots[0].clip.is_none());
        assert!(p.arrangement.lanes.last().unwrap().clips.is_empty());
        let channel = p.channel(track.channel.unwrap()).unwrap();
        if kind == "midi" {
            assert_eq!(channel.volume, 0.2);
            assert_eq!(channel.devices.len(), 1);
            assert_eq!(channel.devices[0].kind, DeviceKind::Builtin("fm-synth".into()));
            assert!(channel.devices[0].state.is_none());
        } else {
            assert!(channel.devices.is_empty());
        }
        assert!(track.color.is_some());
        let mut copy = Session::new(before.clone());
        copy.execute("track.createShared", &params).unwrap();
        copy.execute("edit.undo", &json!({})).unwrap();
        let mut expected = before;
        expected.next_id = copy.project().next_id;
        assert_eq!(copy.project(), &expected);
    }
}

#[test]
fn invalid_shared_track_requests_are_atomic_and_cannot_load_another_device() {
    let mut session = Session::new(Project::default());
    let before = session.project().clone();
    for params in [
        json!({"kind":"group","name":"Group"}),
        json!({"kind":"audio","name":"  "}),
        json!({"kind":"audio","name":"Line\nBreak"}),
        json!({"kind":"midi","name":"é".repeat(129)}),
        json!({"kind":"audio","name":"Voice","device":"sampler"}),
        json!({"kind":"midi","name":"Keys","preset":"/do-not-read.fxp"}),
    ] {
        assert!(session.execute("track.createShared", &params).is_err());
        assert_eq!(session.project(), &before);
    }
}

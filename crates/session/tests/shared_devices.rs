// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;
use suhina_model::{Clip, ContentId, Device, DeviceKind, DeviceParam, Note};
use suhina_session::{Player, Session};
use suhina_test_support::ProjectBuilder;

fn session() -> Result<Session, suhina_session::CommandError> {
    let project = ProjectBuilder::new()
        .midi_track(1, "Keys")
        .notes(1, "Held", 32.0, vec![Note::new(0.0, 32.0, 60, 0.8)])
        .clip(1, Clip::new(ContentId(1), 0.0, 32.0))
        .build();
    let mut session = Session::new(project);
    session.execute("device.add", &json!({"track":1,"id":"fm-synth"}))?;
    session.execute("device.add", &json!({"track":1,"id":"utility"}))?;
    Ok(session)
}

#[test]
fn shared_parameters_accept_native_canonical_values_and_preserve_undo_and_file_roundtrips() {
    let mut s = session().unwrap();
    let ids = s.project().next_id;
    for (device, kind) in [(0, "fm-synth"), (1, "utility")] {
        for p in suhina_device_registry::info(kind).unwrap().params {
            for value in [p.min, p.max, p.default] {
                s.execute("device.setSharedParam", &json!({"track":1,"device":device,"param":p.id,"value":value}))
                    .unwrap();
                assert_eq!(
                    s.project().channels[0].devices[device].params.iter().find(|v| v.id == p.id).unwrap().value,
                    value
                );
            }
        }
    }
    assert_eq!(s.project().next_id, ids);
    let before = s.project().clone();
    s.execute("device.setSharedParam", &json!({"track":1,"device":1,"param":"gain","value":-6})).unwrap();
    let parsed = suhina_project_file::from_json(&suhina_project_file::to_json(s.project())).unwrap();
    assert_eq!(&parsed, s.project());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.project(), &before);
}

#[test]
fn shared_parameters_reach_every_built_in_device() {
    let mut p = session().unwrap().project().clone();
    p.channels[0].devices.push(Device::new("Reverb", DeviceKind::Builtin("reverb".into())));
    let index = p.channels[0].devices.len() - 1;
    let info = suhina_device_registry::info("reverb").unwrap();
    let spec = &info.params[0];
    let value = spec.clamp(spec.max);
    let mut s = Session::new(p);
    s.execute("device.setSharedParam", &json!({"track":1,"device":index,"param":spec.id,"value":value})).unwrap();
    assert!(s.project().channels[0].devices[index].params.iter().any(|x| x.id == spec.id && x.value == value));
}

#[test]
fn shared_parameters_refuse_bad_types_paths_values_and_opaque_or_other_devices_atomically() {
    let mut s = session().unwrap();
    let good = json!({"track":1,"device":0,"param":"algorithm","value":8});
    let before = s.project().clone();
    for (key, value) in [
        ("track", json!("Keys")),
        ("track", json!(0)),
        ("track", json!(u64::MAX)),
        ("track", json!(999)),
        ("device", json!(-1)),
        ("device", json!(16)),
        ("device", json!("0/0/0")),
        ("device", json!(1.5)),
        ("param", json!("unknown")),
        ("param", json!("../voice.syx")),
        ("param", json!("x".repeat(65))),
        ("value", json!(0)),
        ("value", json!(33)),
        ("value", json!(4.5)),
        ("value", json!(null)),
        ("value", json!(1e300)),
        ("preset", json!("/must-not-read.syx")),
        ("continuous", json!(true)),
        ("gesture", json!("drag")),
    ] {
        let mut p = good.clone();
        p[key] = value;
        assert!(s.execute("device.setSharedParam", &p).is_err(), "accepted invalid field {key}");
        assert_eq!(s.project(), &before);
    }
    for kind in [
        DeviceKind::Builtin("sampler".into()),
        DeviceKind::Builtin("overdrive".into()),
        DeviceKind::Sd,
        DeviceKind::Clap { id: "/must-not-start.clap".into() },
        DeviceKind::Vst3 { id: "/must-not-start.vst3".into() },
    ] {
        let mut p = before.clone();
        p.channels[0].devices[0] = Device::new("Unsupported", kind);
        let mut s = Session::new(p.clone());
        assert!(s.execute("device.setSharedParam", &good).is_err());
        assert_eq!(s.project(), &p);
    }
    // A built-in's parameters can be set whatever its state holds, and the state stays as it was.
    let mut p = before.clone();
    p.channels[0].devices[0].state = Some(vec![1, 2, 3]);
    let mut s = Session::new(p.clone());
    s.execute("device.setSharedParam", &good).unwrap();
    assert_eq!(s.project().channels[0].devices[0].state, Some(vec![1, 2, 3]));
    let mut p = before.clone();
    p.channels[0].devices[0].params = (0..512).map(|i| DeviceParam::new(format!("unknown{i}"), 0.)).collect();
    let mut s = Session::new(p.clone());
    assert!(s.execute("device.setSharedParam", &good).is_err());
    assert_eq!(s.project(), &p);
}

#[test]
fn shared_manual_parameters_override_only_their_automation_and_keep_disabled_devices_off() {
    let mut s = session().unwrap();
    for param in ["gain", "width"] {
        s.execute("device.setAutomation", &json!({"track":1,"device":1,"param":param,"points":[0,0,8,1]})).unwrap();
    }
    s.execute("device.setEnabled", &json!({"track":1,"device":1,"enabled":false})).unwrap();
    let before = s.project().clone();
    s.execute("device.setSharedParam", &json!({"track":1,"device":1,"param":"gain","value":-6})).unwrap();
    let device = &s.project().channels[0].devices[1];
    assert!(!device.enabled);
    assert!(!device.automation.iter().find(|a| a.param == "gain").unwrap().enabled);
    assert!(device.automation.iter().find(|a| a.param == "width").unwrap().enabled);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.project(), &before);
}

#[test]
fn shared_parameter_messages_sound_like_native_edits_during_a_held_note_without_resetting_players() {
    use suhina_engine::{AudioPool, Renderer};
    let mut shared = session().unwrap();
    let mut ordinary = Session::new(shared.project().clone());
    let mut a = Renderer::stopped(shared.project(), &AudioPool::default(), 8000).unwrap();
    let mut b = Renderer::stopped(ordinary.project(), &AudioPool::default(), 8000).unwrap();
    shared.attach_player(Player::new(a.controller(), None));
    ordinary.attach_player(Player::new(b.controller(), None));
    for s in [&mut shared, &mut ordinary] {
        s.execute("transport.play", &json!({})).unwrap();
    }
    let mut aa = [0.; 256];
    let mut bb = [0.; 256];
    a.process(&mut aa);
    b.process(&mut bb);
    assert_eq!(aa, bb);
    assert!(aa.iter().any(|x| x.abs() > 0.0001));
    for (device, param, value) in [(0, "volume", -12.), (1, "gain", -6.), (0, "feedback", 3.)] {
        let p = json!({"track":1,"device":device,"param":param,"value":value});
        shared.execute("device.setSharedParam", &p).unwrap();
        ordinary.execute("device.setParam", &p).unwrap();
        for _ in 0..8 {
            a.process(&mut aa);
            b.process(&mut bb);
            assert_eq!(aa, bb);
        }
        assert!(a.monitor().running() && b.monitor().running());
        assert_eq!(a.monitor().beat(), b.monitor().beat());
    }
    assert_eq!(shared.project(), ordinary.project());
    assert!(a.monitor().beat() > 0.1);
}

#[test]
fn shared_project_gate_admits_only_the_guarded_parameter_command_and_keeps_its_checks() {
    let mut projects = suhina_session::Projects::new();
    let id = projects.add(session().unwrap(), None).unwrap();
    projects.restrict_shared_project(id, true).unwrap();
    let good = json!({"track":1,"device":0,"param":"volume","value":-9});
    let before = projects.get(id).unwrap().project().clone();
    for command in ["device.setParam", "device.setSharedParam", "device.add", "edit.undo"] {
        assert!(projects.execute_in(Some(id), command, &good).is_err());
        assert_eq!(projects.get(id).unwrap().project(), &before);
    }
    assert!(projects.apply_shared_control(id, "device.setParam", &good).is_err());
    let mut bad = good.clone();
    bad["device"] = json!("0/0/0");
    assert!(projects.apply_shared_control(id, "device.setSharedParam", &bad).is_err());
    assert_eq!(projects.get(id).unwrap().project(), &before);
    projects.apply_shared_control(id, "device.setSharedParam", &good).unwrap();
    assert_eq!(projects.get(id).unwrap().project().channels[0].devices[0].params[0], DeviceParam::new("volume", -9.));
}

#[test]
fn shared_chain_placement_removal_undo_and_save_preserve_notes_inputs_and_other_device_settings() {
    let mut s = session().unwrap();
    let original = s.project().clone();
    s.execute("device.removeShared", &json!({"track":1,"device":0})).unwrap();
    assert_eq!(s.project().channels[0].devices[0].kind, DeviceKind::Builtin("utility".into()));
    s.execute("device.setSharedParam", &json!({"track":1,"device":0,"param":"gain","value":-9})).unwrap();
    s.execute("device.addShared", &json!({"track":1,"kind":"fm-synth"})).unwrap();
    s.execute("device.addShared", &json!({"track":1,"kind":"utility"})).unwrap();
    let chain = &s.project().channels[0].devices;
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].kind, DeviceKind::Builtin("fm-synth".into()));
    assert_eq!(chain[1].params, vec![DeviceParam::new("gain", -9.)]);
    assert!(chain[2].params.is_empty() && chain[2].enabled && chain[2].state.is_none());
    assert_eq!(s.project().tracks, original.tracks);
    assert_eq!(s.project().contents, original.contents);
    assert_eq!(s.project().arrangement, original.arrangement);
    assert_eq!(s.project().next_id, original.next_id);
    let before = s.project().clone();
    assert!(s.execute("device.addShared", &json!({"track":1,"kind":"fm-synth"})).is_err());
    assert_eq!(s.project(), &before);
    let parsed = suhina_project_file::from_json(&suhina_project_file::to_json(s.project())).unwrap();
    assert_eq!(&parsed, s.project());
    for _ in 0..4 {
        s.execute("edit.undo", &json!({})).unwrap();
    }
    assert_eq!(s.project(), &original);
    s.execute("device.setEnabled", &json!({"track":1,"device":0,"enabled":false})).unwrap();
    let before = s.project().clone();
    assert!(s.execute("device.addShared", &json!({"track":1,"kind":"fm-synth"})).is_err());
    assert_eq!(s.project(), &before, "disabled instruments must not be replaced");
}

#[test]
fn shared_chain_guards_reject_paths_unknown_kinds_opaque_chains_wrong_indices_and_audio_instruments() {
    let mut s = session().unwrap();
    let before = s.project().clone();
    for (command, params) in [
        ("device.addShared", json!({"track":"Keys","kind":"utility"})),
        ("device.addShared", json!({"track":1,"kind":"sampler"})),
        ("device.addShared", json!({"track":1,"kind":"utility","preset":"/must-not-read.syx"})),
        ("device.addShared", json!({"track":1,"kind":"utility","position":0})),
        ("device.addShared", json!({"track":1,"kind":"utility","state":[1]})),
        ("device.addShared", json!({"track":0,"kind":"utility"})),
        ("device.removeShared", json!({"track":1,"device":"0/0/0"})),
        ("device.removeShared", json!({"track":1,"device":16})),
        ("device.removeShared", json!({"track":1,"device":-1})),
        ("device.removeShared", json!({"track":1,"device":2})),
        ("device.removeShared", json!({"track":1,"device":0,"continue":true})),
    ] {
        assert!(s.execute(command, &params).is_err());
        assert_eq!(s.project(), &before);
    }
    for opaque in [false, true] {
        let mut p = before.clone();
        if opaque {
            p.channels[0].devices[0].state = Some(vec![1]);
        } else {
            p.channels[0].devices[0] =
                Device::new("Unsupported", DeviceKind::Clap { id: "/must-not-load.clap".into() });
        }
        let mut s = Session::new(p.clone());
        // Neither the plug-in nor a built-in holding state is removed, and FM Synth never joins
        // a chain that may already hold an instrument.
        for (id, params) in [
            ("device.removeShared", json!({"track":1,"device":0})),
            ("device.addShared", json!({"track":1,"kind":"fm-synth"})),
        ] {
            assert!(s.execute(id, &params).is_err());
            assert_eq!(s.project(), &p);
        }
        // Utility still joins and leaves such a chain, which keeps the plug-in untouched.
        s.execute("device.addShared", &json!({"track":1,"kind":"utility"})).unwrap();
        assert_eq!(s.project().channels[0].devices[0], p.channels[0].devices[0]);
        let last = s.project().channels[0].devices.len() - 1;
        s.execute("device.removeShared", &json!({"track":1,"device":last})).unwrap();
        assert_eq!(s.project(), &p);
    }
    let mut audio = Session::new(ProjectBuilder::new().audio_track(1, "Audio").build());
    let before = audio.project().clone();
    assert!(audio.execute("device.addShared", &json!({"track":1,"kind":"fm-synth"})).is_err());
    assert_eq!(audio.project(), &before);
    audio.execute("device.addShared", &json!({"track":1,"kind":"utility"})).unwrap();
}

#[test]
fn shared_chain_channel_and_project_quotas_fail_without_mutation() {
    let mut project = ProjectBuilder::new().midi_track(1, "Full").build();
    project.channels[0].devices =
        (0..16).map(|_| Device::new("Utility", DeviceKind::Builtin("utility".into()))).collect();
    let mut s = Session::new(project.clone());
    assert!(s.execute("device.addShared", &json!({"track":1,"kind":"utility"})).is_err());
    assert_eq!(s.project(), &project);
    let mut builder = ProjectBuilder::new();
    for id in 1..=9 {
        builder = builder.midi_track(id, &format!("Track {id}"));
    }
    let mut p = builder.build();
    for c in p.channels.iter_mut().take(8) {
        c.devices = project.channels[0].devices.clone();
    }
    let mut s = Session::new(p.clone());
    assert!(s.execute("device.addShared", &json!({"track":9,"kind":"utility"})).is_err());
    assert_eq!(s.project(), &p);
}

#[test]
fn shared_chain_graph_updates_match_native_audio_without_replacing_players() {
    use suhina_engine::{AudioPool, Renderer};
    let project = ProjectBuilder::new()
        .midi_track(1, "Keys")
        .midi_track(2, "Unchanged")
        .notes(1, "Held", 32., vec![Note::new(0., 32., 60, 0.8)])
        .clip(1, Clip::new(ContentId(1), 0., 32.))
        .notes(2, "Held", 32., vec![Note::new(0., 32., 48, 0.6)])
        .clip(2, Clip::new(ContentId(2), 0., 32.))
        .build();
    let mut shared = Session::new(project);
    shared.execute("device.add", &json!({"track":1,"id":"fm-synth"})).unwrap();
    shared.execute("device.add", &json!({"track":1,"id":"utility"})).unwrap();
    let mut ordinary = Session::new(shared.project().clone());
    let mut a = Renderer::stopped(shared.project(), &AudioPool::default(), 8000).unwrap();
    let mut b = Renderer::stopped(ordinary.project(), &AudioPool::default(), 8000).unwrap();
    shared.attach_player(Player::new(a.controller(), None));
    ordinary.attach_player(Player::new(b.controller(), None));
    for s in [&mut shared, &mut ordinary] {
        s.execute("transport.play", &json!({})).unwrap();
    }
    let mut aa = [0.; 256];
    let mut bb = [0.; 256];
    a.process(&mut aa);
    b.process(&mut bb);
    for (guarded, normal, p, q) in [
        ("device.removeShared", "device.remove", json!({"track":1,"device":0}), json!({"track":1,"device":0})),
        ("device.addShared", "device.add", json!({"track":1,"kind":"fm-synth"}), json!({"track":1,"id":"fm-synth"})),
        ("device.addShared", "device.add", json!({"track":1,"kind":"utility"}), json!({"track":1,"id":"utility"})),
        ("device.removeShared", "device.remove", json!({"track":1,"device":1}), json!({"track":1,"device":1})),
    ] {
        shared.execute(guarded, &p).unwrap();
        ordinary.execute(normal, &q).unwrap();
        for _ in 0..8 {
            a.process(&mut aa);
            b.process(&mut bb);
            assert_eq!(aa, bb);
        }
        assert!(a.monitor().running() && b.monitor().running());
        assert_eq!(a.monitor().beat(), b.monitor().beat());
        assert_eq!(shared.project(), ordinary.project());
    }
    assert!(aa.iter().any(|x| x.abs() > 0.0001));
}

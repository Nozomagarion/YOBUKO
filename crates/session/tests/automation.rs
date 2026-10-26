// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Automation commands (#395): points on mixer and device lanes, undo, turning a lane off by a
//! manual change and on again, and malformed parameters.

use serde_json::json;
use suhina_model::*;
use suhina_session::Session;

use crate::support::run;
use suhina_test_support::ProjectBuilder;

fn project() -> Project {
    suhina_test_support::with_ids(project_built())
}

fn project_built() -> Project {
    ProjectBuilder::new().midi_track(1, "Keys").build_without_ids()
}

fn lanes(session: &Session) -> &[AutomationLane] {
    &session.project().channels[0].automation
}

fn sd_device() -> Device {
    let mut graph =
        suhina_device_sdk::DeviceGraph::new("test.gain", "Test Gain", suhina_device_sdk::DeviceRole::Effect);
    graph.parameters.push(suhina_device_sdk::Parameter::new("gain", "Gain", (0.0, 2.0), 0.5));
    graph.nodes = vec![
        suhina_device_sdk::Node::audio_input("in"),
        suhina_device_sdk::Node::gain("gain", "gain"),
        suhina_device_sdk::Node::audio_output("out"),
    ];
    graph.connections =
        vec![suhina_device_sdk::Connection::new("in", "gain"), suhina_device_sdk::Connection::new("gain", "out")];
    Device {
        name: "Test Gain".into(),
        kind: DeviceKind::Sd,
        enabled: true,
        state: graph.to_sd().ok().map(String::into_bytes),
        params: Vec::new(),
        automation: Vec::new(),
        clip_host: false,
        extensions: Extensions::default(),
    }
}

#[test]
fn mixer_points_are_added_moved_and_deleted_with_undo() {
    let mut session = Session::new(project());
    let added =
        run(&mut session, "automation.addPoint", json!({ "track": "Keys", "param": "volume", "beat": 4, "value": 0 }))
            .unwrap();
    assert_eq!((added["point"].as_u64(), added["points"].as_u64()), (Some(0), Some(1)));
    // In beat order: a point before the first goes first.
    let added = run(
        &mut session,
        "automation.addPoint",
        json!({ "track": 1, "param": "volume", "beat": 0, "value": -150.5, "curve": 0.5 }),
    )
    .unwrap();
    assert_eq!(added["point"], 0);
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 2, "value": -1 })).unwrap();
    let volume = &lanes(&session)[0];
    assert_eq!(volume.param, "volume");
    assert_eq!(volume.points, [AutomationPoint { beat: 0.0, value: 0.0, curve: 0.5 }, AutomationPoint::new(4.0, 1.0)]);

    // Moving the first point past the second puts it second.
    let moved = run(
        &mut session,
        "automation.movePoint",
        json!({ "track": 1, "param": "volume", "point": 0, "beat": 8, "value": -6 }),
    )
    .unwrap();
    assert_eq!(moved["point"], 1);
    assert_eq!(moved["value"], -6.0);
    assert_eq!(lanes(&session)[0].points[1].beat, 8.0);
    assert_eq!(lanes(&session)[0].points[1].curve, 0.5, "the curve stays");

    let list = run(&mut session, "automation.list", json!({ "track": 1 })).unwrap();
    assert_eq!(list["mixer"][0]["points"][1]["value"], -6.0, "the volume in dB");
    assert_eq!(list["mixer"][1]["param"], "pan");
    assert_eq!(list["devices"], json!([]));

    // Deleting the last point removes the lane.
    run(&mut session, "automation.deletePoint", json!({ "track": 1, "param": "pan", "point": 0 })).unwrap();
    assert_eq!(lanes(&session).len(), 1);
    let cleared = run(&mut session, "automation.clear", json!({ "track": 1 })).unwrap();
    assert_eq!(cleared["cleared"], 1);
    assert!(lanes(&session).is_empty());

    // Every edit was one undo step.
    for _ in 0..6 {
        run(&mut session, "edit.undo", json!({})).unwrap();
    }
    assert_eq!(session.project(), &project());
}

#[test]
fn a_manual_change_turns_a_lane_off_until_it_is_on_again() {
    let mut session = Session::new(project());
    run(&mut session, "device.add", json!({ "track": 1, "id": "utility" })).unwrap();
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "volume", "beat": 0, "value": -12 }))
        .unwrap();
    run(
        &mut session,
        "automation.addPoint",
        json!({ "track": 1, "device": 0, "param": "gain", "beat": 0, "value": 99 }),
    )
    .unwrap();
    // Kept in the parameter's range.
    assert_eq!(session.project().channels[0].devices[0].automation[0].points[0].value, 35.0);

    let set = run(&mut session, "track.setVolume", json!({ "tracks": [1], "db": -3 })).unwrap();
    assert_eq!(set["automation"], "off");
    assert!(!lanes(&session)[0].enabled);
    // Once off, another change says nothing more.
    let set = run(&mut session, "track.setVolume", json!({ "tracks": [1], "db": -4 })).unwrap();
    assert!(set.get("automation").is_none());
    let set =
        run(&mut session, "device.setParam", json!({ "track": 1, "device": 0, "param": "gain", "value": -6 })).unwrap();
    assert_eq!(set["automation"], "off");
    assert!(!session.project().channels[0].devices[0].automation[0].enabled);
    // A parameter without a lane is not touched.
    let set = run(&mut session, "track.setPan", json!({ "tracks": [1], "pan": 0.5 })).unwrap();
    assert!(set.get("automation").is_none());

    let list = run(&mut session, "automation.list", json!({ "track": 1 })).unwrap();
    assert_eq!(list["mixer"][0]["enabled"], false);
    assert_eq!(list["devices"][0]["lanes"][0]["enabled"], false);
    run(&mut session, "automation.setEnabled", json!({ "track": 1, "param": "volume", "enabled": true })).unwrap();
    run(&mut session, "automation.setEnabled", json!({ "track": 1, "device": 0, "param": "gain", "enabled": true }))
        .unwrap();
    assert!(lanes(&session)[0].enabled);
    assert!(session.project().channels[0].devices[0].automation[0].enabled);
    // The value set by hand stays, under the lane.
    assert_eq!(session.project().channels[0].devices[0].params, [DeviceParam::new("gain", -6.0)]);
}

#[test]
fn device_set_automation_takes_built_in_devices() {
    let mut session = Session::new(project());
    run(&mut session, "device.add", json!({ "track": 1, "id": "utility" })).unwrap();
    let set = run(
        &mut session,
        "device.setAutomation",
        json!({ "track": 1, "device": 0, "param": "gain", "points": [4, -6, 0, -100] }),
    )
    .unwrap();
    assert_eq!((set["param"].as_str(), set["points"].as_u64()), (Some("gain"), Some(2)));
    let lane = &session.project().channels[0].devices[0].automation[0];
    // In beat order, limited to the range.
    assert_eq!(lane.points.iter().map(|p| (p.beat, p.value)).collect::<Vec<_>>(), [(0.0, -70.0), (4.0, -6.0)]);
    assert!(
        run(&mut session, "device.setAutomation", json!({ "track": 1, "device": 0, "param": "nope", "points": [] }))
            .is_err()
    );
}

#[test]
fn sd_automation_uses_stable_ids_and_the_parameter_range() {
    let mut p = project();
    p.channels[0].devices.push(sd_device());
    let mut session = Session::new(p);
    let set = run(
        &mut session,
        "device.setAutomation",
        json!({ "track": 1, "device": 0, "param": "gain", "points": [4, 3, 0, -1] }),
    )
    .unwrap();
    assert_eq!(set["param"], "gain");
    assert_eq!(
        session.project().channels[0].devices[0].automation[0]
            .points
            .iter()
            .map(|p| (p.beat, p.value))
            .collect::<Vec<_>>(),
        [(0.0, 0.0), (4.0, 2.0)]
    );
    run(
        &mut session,
        "automation.addPoint",
        json!({ "track": 1, "device": 0, "param": "gain", "beat": 2, "value": 1.25 }),
    )
    .unwrap();
    assert_eq!(
        run(&mut session, "automation.list", json!({ "track": 1 })).unwrap()["devices"][0]["lanes"][0]["param"],
        "gain"
    );
    let set =
        run(&mut session, "device.setParam", json!({ "track": 1, "device": 0, "param": "gain", "value": 1 })).unwrap();
    assert_eq!(set["automation"], "off");
    assert!(session.project().channels[0].devices[0].automation.iter().all(|lane| !lane.enabled));
}

#[test]
fn malformed_automation_edits_are_refused_and_change_nothing() {
    let mut session = Session::new(project());
    run(&mut session, "device.add", json!({ "track": 1, "id": "utility" })).unwrap();
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 })).unwrap();
    let before = session.project().clone();
    for (id, params) in [
        ("automation.addPoint", json!({ "track": 1, "param": "loudness", "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 9, "param": "pan", "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": -1, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 1e300, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": "soon", "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 2 })),
        ("automation.addPoint", json!({ "track": 1, "param": "volume", "beat": 0, "value": 7 })),
        ("automation.addPoint", json!({ "track": 1, "param": "mute", "beat": 0, "value": 0.5, "curve": 2 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0, "curve": "round" })),
        ("automation.addPoint", json!({ "track": 1, "device": 1, "param": "gain", "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "device": 0, "param": "volume", "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "device": -1, "param": "gain", "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "beat": 0, "value": 0 })),
        ("automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0, "colour": 1 })),
        ("automation.movePoint", json!({ "track": 1, "param": "pan", "point": 1, "beat": 2 })),
        ("automation.movePoint", json!({ "track": 1, "param": "pan", "point": 0 })),
        ("automation.movePoint", json!({ "track": 1, "param": "volume", "point": 0, "beat": 2 })),
        ("automation.deletePoint", json!({ "track": 1, "param": "pan", "point": 5 })),
        ("automation.deletePoint", json!({ "track": 1, "param": "pan", "point": "first" })),
        ("automation.setEnabled", json!({ "track": 1, "param": "volume", "enabled": false })),
        ("automation.setEnabled", json!({ "track": 1, "param": "pan", "enabled": "no" })),
        ("automation.clear", json!({ "track": 1, "device": 4 })),
        ("automation.list", json!({ "track": "Nobody" })),
    ] {
        assert!(run(&mut session, id, params.clone()).is_err(), "{id} {params}");
        assert_eq!(session.project(), &before, "{id} {params}");
    }
    // One undo step for each good edit (the device and the point).
    run(&mut session, "edit.undo", json!({})).unwrap();
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert!(run(&mut session, "edit.undo", json!({})).is_err());
}

#[test]
fn a_lane_holds_a_limited_number_of_points() {
    let mut p = project();
    let mut lane = AutomationLane::new("pan");
    lane.points = (0..automation::MAX_POINTS).map(|i| AutomationPoint::new(i as f64 * 0.001, 0.0)).collect();
    p.channels[0].automation.push(lane);
    let mut session = Session::new(p);
    let full = run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 }));
    assert!(full.unwrap_err().contains("at most"));
}

#[test]
fn several_points_move_together_in_one_undo_step() {
    let mut session = Session::new(project());
    for (beat, value) in [(0, -12), (4, -6), (8, 0)] {
        run(
            &mut session,
            "automation.addPoint",
            json!({ "track": 1, "param": "volume", "beat": beat, "value": value }),
        )
        .unwrap();
    }
    let before = session.project().clone();
    // The first two move two beats later and 3 dB up; the first passes no other point.
    let moved = run(
        &mut session,
        "automation.movePoints",
        json!({ "track": 1, "param": "volume", "points": [0, 1], "beats": 2, "value": 3 }),
    )
    .unwrap();
    assert_eq!(moved["points"], json!([0, 1]));
    let beats: Vec<f64> = lanes(&session)[0].points.iter().map(|p| p.beat).collect();
    assert_eq!(beats, [2.0, 6.0, 8.0]);
    let db = |v: f64| (20.0 * v.log10() * 100.0).round() / 100.0;
    assert_eq!(db(lanes(&session)[0].points[1].value), -3.0);
    // The top of the fader holds a point that would go past it.
    run(&mut session, "automation.movePoints", json!({ "track": 1, "param": "volume", "points": [2], "value": 10 }))
        .unwrap();
    assert!((db(lanes(&session)[0].points[2].value) - 6.0).abs() < 0.01);
    run(&mut session, "edit.undo", json!({})).unwrap();
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(session.project(), &before);
}

#[test]
fn moved_points_keep_beat_order_and_report_their_new_places() {
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, 0.0), (4.0, 0.5), (8.0, -0.5)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    // The first point jumps past the others; values stay in the pan's range.
    let moved = run(
        &mut session,
        "automation.movePoints",
        json!({ "track": 1, "param": "pan", "points": [0], "beats": 10, "value": -2 }),
    )
    .unwrap();
    assert_eq!(moved["points"], json!([2]));
    let points: Vec<(f64, f64)> = lanes(&session)[0].points.iter().map(|p| (p.beat, p.value)).collect();
    assert_eq!(points, [(4.0, 0.5), (8.0, -0.5), (10.0, -1.0)]);
}

/// A point dragged onto the beat of the next one stays before it, so the lane jumps from the
/// dragged point's value to the other's, as drawn while dragging, not the other way round; a
/// point dragged back onto the beat of the one before stays after it, and changing only the
/// value of the first of two points at one beat keeps their order.
#[test]
fn a_point_moved_onto_another_points_beat_keeps_its_side() {
    let pans =
        |session: &Session| -> Vec<(f64, f64)> { lanes(session)[0].points.iter().map(|p| (p.beat, p.value)).collect() };
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, -0.5), (4.0, 0.5)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    let moved = run(&mut session, "automation.movePoint", json!({ "track": 1, "param": "pan", "point": 0, "beat": 4 }))
        .unwrap();
    assert_eq!(moved["point"], json!(0));
    assert_eq!(pans(&session), [(4.0, -0.5), (4.0, 0.5)]);
    // Only the value of the first: the order stays.
    run(&mut session, "automation.movePoint", json!({ "track": 1, "param": "pan", "point": 0, "value": -0.25 }))
        .unwrap();
    assert_eq!(pans(&session), [(4.0, -0.25), (4.0, 0.5)]);
    // A point that comes from later goes after the ones at its beat.
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 8, "value": 1 })).unwrap();
    run(&mut session, "automation.movePoint", json!({ "track": 1, "param": "pan", "point": 2, "beat": 4 })).unwrap();
    assert_eq!(pans(&session), [(4.0, -0.25), (4.0, 0.5), (4.0, 1.0)]);
    // A selection moved later goes before the points at its beat, and a value change of a
    // selection keeps the order.
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, -0.5), (4.0, 0.5)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    run(&mut session, "automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0], "beats": 4 }))
        .unwrap();
    assert_eq!(pans(&session), [(4.0, -0.5), (4.0, 0.5)]);
    let moved =
        run(&mut session, "automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0], "value": 0.25 }))
            .unwrap();
    assert_eq!(moved["points"], json!([0]));
    assert_eq!(pans(&session), [(4.0, -0.25), (4.0, 0.5)]);
}

/// Two selected points at one beat (a jump) moved together onto a third point's beat keep their
/// order, whatever order the request names them in, and the places reported follow the request.
#[test]
fn points_at_one_beat_moved_together_keep_their_jump() {
    let pans =
        |session: &Session| -> Vec<(f64, f64)> { lanes(session)[0].points.iter().map(|p| (p.beat, p.value)).collect() };
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, -0.5), (2.0, 0.5), (4.0, 1.0)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    // A jump at beat 0, from -0.5 to 0.5.
    run(&mut session, "automation.movePoint", json!({ "track": 1, "param": "pan", "point": 1, "beat": 0 })).unwrap();
    assert_eq!(pans(&session), [(0.0, -0.5), (0.0, 0.5), (4.0, 1.0)]);
    // Moved later onto the third point's beat, named last first.
    let moved =
        run(&mut session, "automation.movePoints", json!({ "track": 1, "param": "pan", "points": [1, 0], "beats": 4 }))
            .unwrap();
    assert_eq!(moved["points"], json!([1, 0]));
    assert_eq!(pans(&session), [(4.0, -0.5), (4.0, 0.5), (4.0, 1.0)]);
    // And back earlier onto a point at beat 0: after it, still in their order.
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 })).unwrap();
    assert_eq!(pans(&session)[0], (0.0, 0.0));
    let moved = run(
        &mut session,
        "automation.movePoints",
        json!({ "track": 1, "param": "pan", "points": [2, 1], "beats": -4 }),
    )
    .unwrap();
    assert_eq!(moved["points"], json!([2, 1]));
    assert_eq!(pans(&session), [(0.0, 0.0), (0.0, -0.5), (0.0, 0.5), (4.0, 1.0)]);
}

#[test]
fn several_points_are_deleted_in_one_undo_step_and_the_lane_goes_with_the_last() {
    let mut session = Session::new(project());
    for beat in [0, 2, 4] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "mute", "beat": beat, "value": 1 }))
            .unwrap();
    }
    run(&mut session, "automation.deletePoints", json!({ "track": 1, "param": "mute", "points": [2, 0] })).unwrap();
    assert_eq!(lanes(&session)[0].points.len(), 1);
    assert_eq!(lanes(&session)[0].points[0].beat, 2.0);
    run(&mut session, "automation.deletePoints", json!({ "track": 1, "param": "mute", "points": [0] })).unwrap();
    assert!(lanes(&session).is_empty());
    run(&mut session, "edit.undo", json!({})).unwrap();
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(lanes(&session)[0].points.len(), 3);
}

#[test]
fn malformed_point_lists_change_nothing() {
    let mut session = Session::new(project());
    for beat in [0, 2] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": 0 }))
            .unwrap();
    }
    let before = session.project().clone();
    for (id, params) in [
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0], "beats": -1 })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0, 0], "beats": 1 })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [5], "beats": 1 })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0] })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [], "beats": 1 })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [-1], "beats": 1 })),
        ("automation.movePoints", json!({ "track": 1, "param": "pan", "points": [0], "curve": 2 })),
        ("automation.deletePoints", json!({ "track": 1, "param": "pan", "points": [2] })),
        ("automation.deletePoints", json!({ "track": 1, "param": "pan", "points": "0" })),
        ("automation.deletePoints", json!({ "track": 1, "param": "volume", "points": [0] })),
    ] {
        assert!(run(&mut session, id, params.clone()).is_err(), "{id} {params}");
    }
    assert_eq!(session.project(), &before);
}

fn pan_points(session: &Session) -> Vec<(f64, f64)> {
    lanes(session).iter().find(|l| l.param == "pan").map_or_else(Vec::new, |l| {
        l.points.iter().map(|p| ((p.beat * 1e6).round() / 1e6, (p.value * 1e6).round() / 1e6)).collect()
    })
}

/// A range of a lane copies with its edge values and pastes anywhere, onto another parameter at
/// the same heights; Ctrl+D's duplicate goes right after the range; a deleted range keeps the
/// envelope outside it (#916).
#[test]
fn an_automation_range_copies_pastes_duplicates_and_deletes() {
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, -1.0), (4.0, 1.0)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    run(&mut session, "automation.copy", json!({ "track": 1, "param": "pan", "start": 0, "end": 4 })).unwrap();
    // Pasted at beat 8: the rise again, the lane holding its last value up to there.
    let done = run(&mut session, "automation.paste", json!({ "track": 1, "param": "pan", "start": 8 })).unwrap();
    assert_eq!((done["start"].as_f64(), done["end"].as_f64()), (Some(8.0), Some(12.0)));
    assert_eq!(pan_points(&session), [(0.0, -1.0), (4.0, 1.0), (8.0, 1.0), (8.0, -1.0), (12.0, 1.0)]);
    // Onto the mute lane: the bottom is off, the top on.
    run(&mut session, "automation.paste", json!({ "track": 1, "param": "mute", "start": 0 })).unwrap();
    let mute = lanes(&session).iter().find(|l| l.param == "mute").unwrap();
    assert_eq!(mute.points.first().map(|p| p.value), Some(0.0));
    assert_eq!(mute.points.last().map(|p| p.value), Some(1.0));
    // Duplicated right after itself, answering the copy's range.
    let done = run(&mut session, "automation.duplicate", json!({ "track": 1, "param": "pan", "start": 8, "end": 12 }))
        .unwrap();
    assert_eq!((done["start"].as_f64(), done["end"].as_f64()), (Some(12.0), Some(16.0)));
    let points = pan_points(&session);
    assert!(points.contains(&(16.0, 1.0)) && points.contains(&(12.0, -1.0)), "{points:?}");
    // Deleting 1 to 3 keeps the values at 1 and 3, so the line outside is as it was.
    run(&mut session, "automation.deleteRange", json!({ "track": 1, "param": "pan", "start": 1, "end": 3 })).unwrap();
    assert_eq!(pan_points(&session)[..4], [(0.0, -1.0), (1.0, -0.5), (3.0, 0.5), (4.0, 1.0)]);
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(pan_points(&session)[..2], [(0.0, -1.0), (4.0, 1.0)]);
}

/// A shape fills a range over the parameter's whole travel, a repeating one as many times as
/// asked; the envelope outside the range stays (#916).
#[test]
fn a_shape_fills_an_automation_range() {
    let mut session = Session::new(project());
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 })).unwrap();
    run(
        &mut session,
        "automation.insertShape",
        json!({ "track": 1, "param": "pan", "start": 4, "end": 8, "shape": "square", "cycles": 2 }),
    )
    .unwrap();
    assert_eq!(
        pan_points(&session),
        [
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 1.0),
            (5.0, 1.0),
            (5.0, -1.0),
            (6.0, -1.0),
            (6.0, 1.0),
            (7.0, 1.0),
            (7.0, -1.0),
            (8.0, -1.0),
            (8.0, 0.0)
        ]
    );
    // A shape on a lane with no automation makes the lane.
    run(
        &mut session,
        "automation.insertShape",
        json!({ "track": 1, "param": "volume", "start": 0, "end": 4, "shape": "ramp_up" }),
    )
    .unwrap();
    let volume = lanes(&session).iter().find(|l| l.param == "volume").unwrap();
    assert_eq!(volume.points.first().map(|p| p.value), Some(0.0), "the bottom is silence");
    assert!(volume.points.last().is_some_and(|p| (p.value - suhina_model::automation::MAX_VOLUME).abs() < 1e-9));
    assert!(
        run(
            &mut session,
            "automation.insertShape",
            json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "shape": "spiral" })
        )
        .is_err()
    );
    assert!(run(&mut session, "automation.copy", json!({ "track": 1, "param": "pan", "start": 4, "end": 4 })).is_err());
}

/// With `point`, a shape fills the segment from the point before it, between the two points'
/// heights; `automation.addPoint` with `shape` gives the new point's segment that shape (#916).
#[test]
fn a_shape_fills_a_points_segment_and_a_new_points_segment() {
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, 0.0), (4.0, 1.0)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    run(&mut session, "automation.insertShape", json!({ "track": 1, "param": "pan", "point": 1, "shape": "square" }))
        .unwrap();
    let points = pan_points(&session);
    assert_eq!(points.first(), Some(&(0.0, 0.0)));
    assert_eq!(points.last(), Some(&(4.0, 1.0)));
    assert!(points.iter().all(|&(b, v)| (0.0..=4.0).contains(&b) && (0.0..=1.0).contains(&v)), "{points:?}");
    assert!(points.len() > 6, "a square once a beat: {points:?}");
    // A new point at 8 with a falling curve: its segment from 4 takes the bent line.
    run(
        &mut session,
        "automation.addPoint",
        json!({ "track": 1, "param": "pan", "beat": 8, "value": -1, "shape": "curve_down" }),
    )
    .unwrap();
    let after: Vec<(f64, f64)> = pan_points(&session).into_iter().filter(|(b, _)| *b >= 4.0).collect();
    assert_eq!(after.last(), Some(&(8.0, -1.0)));
    assert!(lanes(&session).iter().find(|l| l.param == "pan").is_some_and(|l| l.points.iter().any(|p| p.curve > 0.5)));
    assert!(
        run(
            &mut session,
            "automation.insertShape",
            json!({ "track": 1, "param": "pan", "point": 99, "shape": "sine" })
        )
        .is_err()
    );
}

/// Less depth swings a shape around the lane's value where the range starts; a sine is a few bent
/// segments a cycle; Smooth points thins dense points and keeps the line; Stretch scales the
/// points around their middle (#916).
#[test]
fn shapes_take_a_depth_and_ranges_smooth_and_stretch() {
    let mut session = Session::new(project());
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 })).unwrap();
    run(
        &mut session,
        "automation.insertShape",
        json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "shape": "square", "depth": 0.5 }),
    )
    .unwrap();
    let values: Vec<f64> = pan_points(&session).iter().map(|p| p.1).collect();
    assert!(values.iter().all(|v| (-0.5..=0.5).contains(v)), "half the travel around the middle: {values:?}");
    assert!(values.contains(&0.5) && values.contains(&-0.5), "{values:?}");
    // A sine over 4 beats, once a beat: five points a cycle at most, bent.
    run(
        &mut session,
        "automation.insertShape",
        json!({ "track": 1, "param": "pan", "start": 8, "end": 12, "shape": "sine", "cycles": 4 }),
    )
    .unwrap();
    let sine: Vec<(f64, f64)> = pan_points(&session).into_iter().filter(|(b, _)| (8.0..=12.0).contains(b)).collect();
    assert!(sine.len() <= 18, "{sine:?}");
    // Dense points along a straight rise thin to its two ends.
    for i in 0..=20 {
        let beat = 16.0 + f64::from(i) * 0.2;
        let value = -1.0 + f64::from(i) * 0.1;
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    let done = run(&mut session, "automation.simplify", json!({ "track": 1, "param": "pan", "start": 16, "end": 20 }))
        .unwrap();
    assert_eq!(done["removed"], json!(19));
    let line: Vec<(f64, f64)> = pan_points(&session).into_iter().filter(|(b, _)| (16.0..=20.0).contains(b)).collect();
    assert_eq!(line, [(16.0, -1.0), (20.0, 1.0)]);
    // Stretch at a half: the square's swing halves around its middle.
    run(
        &mut session,
        "automation.scaleRange",
        json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "factor": 0.5 }),
    )
    .unwrap();
    let values: Vec<f64> = pan_points(&session).iter().filter(|p| p.0 <= 4.0).map(|p| p.1).collect();
    assert!(values.iter().all(|v| (-0.25 - 1e-9..=0.25 + 1e-9).contains(v)), "{values:?}");
    assert!(
        run(
            &mut session,
            "automation.scaleRange",
            json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "factor": 9 })
        )
        .is_err()
    );
}

/// Stretch on picked points moves only them, around the middle of the line over their span: the
/// troughs of a wave picked alone deepen and its peaks stay (#916).
#[test]
fn stretching_picked_points_moves_only_them() {
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, 0.5), (1.0, -0.5), (2.0, 0.5), (3.0, -0.5), (4.0, 0.5)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    run(&mut session, "automation.scaleRange", json!({ "track": 1, "param": "pan", "points": [1, 3], "factor": 2 }))
        .unwrap();
    let values: Vec<f64> = pan_points(&session).iter().map(|p| p.1).collect();
    assert_eq!(values, [0.5, -1.0, 0.5, -1.0, 0.5]);
}

/// Smooth points keeps a sine a sine: the line after it stays within a few hundredths of the
/// line before it everywhere, curves and all, with fewer points (#916).
#[test]
fn smoothing_a_sine_keeps_its_shape() {
    use suhina_model::automation::{Plain, value_at};
    let mut session = Session::new(project());
    // A sine of 16 straight points a beat, over 4 beats.
    for i in 0..=64 {
        let beat = f64::from(i) / 16.0;
        let value = (std::f64::consts::TAU * beat).sin() * 0.8;
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    let lane = |s: &Session| lanes(s).iter().find(|l| l.param == "pan").unwrap().points.clone();
    let before = lane(&session);
    run(&mut session, "automation.simplify", json!({ "track": 1, "param": "pan", "start": 0, "end": 4 })).unwrap();
    let after = lane(&session);
    assert!(after.len() < before.len() / 2, "{} -> {}", before.len(), after.len());
    for i in 0..=400 {
        let beat = f64::from(i) / 100.0;
        let (old, new) =
            (value_at(&before, beat, false, &Plain).unwrap(), value_at(&after, beat, false, &Plain).unwrap());
        // 2 % of the pan's travel (-1 to 1) is 0.04; a little more for the old straight pieces.
        assert!((old - new).abs() < 0.06, "at beat {beat}: {old} -> {new}");
    }
}

/// Smooth rounds a square's jumps into eased steps that pass the middle at the jump's beat, and a
/// triangle's tip into a curve that stays under it; undo brings the sharp line back (#916).
#[test]
fn smooth_rounds_jumps_and_corners() {
    use suhina_model::automation::{Plain, value_at};
    let mut session = Session::new(project());
    let lane = |s: &Session| lanes(s).iter().find(|l| l.param == "pan").unwrap().points.clone();
    run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": 0, "value": 0 })).unwrap();
    run(
        &mut session,
        "automation.insertShape",
        json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "shape": "square", "cycles": 2 }),
    )
    .unwrap();
    let square = lane(&session);
    let jumps = |points: &[suhina_model::AutomationPoint]| {
        points
            .windows(2)
            .filter(|w| w[0].beat > 0.01 && w[0].beat < 3.99 && (w[1].beat - w[0].beat).abs() < 1e-9)
            .count()
    };
    assert!(jumps(&square) > 0, "{square:?}");
    let done = run(
        &mut session,
        "automation.smooth",
        json!({ "track": 1, "param": "pan", "start": 0, "end": 4, "amount": 1 }),
    )
    .unwrap();
    assert!(done["rounded"].as_u64().unwrap() > 0, "{done}");
    let smooth = lane(&session);
    assert_eq!(jumps(&smooth), 0, "{smooth:?}");
    // At a jump's beat the eased step is halfway; just after it, it has not reached the top yet.
    let jump = square.windows(2).find(|w| w[0].beat > 0.01 && (w[1].beat - w[0].beat).abs() < 1e-9).unwrap();
    let (from, to, at) = (jump[0].value, jump[1].value, jump[0].beat);
    let middle = value_at(&smooth, at, false, &Plain).unwrap();
    assert!((middle - (from + to) / 2.0).abs() < 1e-6, "{middle} between {from} and {to}");
    let soon = value_at(&smooth, at + 0.05, false, &Plain).unwrap();
    assert!((soon - to).abs() > 1e-3 && (soon - from).abs() > (to - from).abs() / 2.0, "{soon}");
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(lane(&session), square);

    // A triangle's tip: the corner stays, the line on either side bends into it.
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, -1.0), (1.0, 1.0), (2.0, -1.0)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    run(&mut session, "automation.smooth", json!({ "track": 1, "param": "pan", "start": 0, "end": 2, "amount": 0.5 }))
        .unwrap();
    let round = lane(&session);
    assert_eq!(round.len(), 5, "{round:?}");
    assert!(round.iter().any(|p| p.beat == 1.0 && p.value == 1.0), "{round:?}");
    let near = value_at(&round, 0.9, false, &Plain).unwrap();
    assert!(near > 0.6 && near < 1.0, "the straight line is at 0.6 there, the rounded one above it: {near}");
}

/// Scale steps that continue one another are one undo step, as a slider's drag is (#916).
#[test]
fn continued_scaling_is_one_undo_step() {
    let mut session = Session::new(project());
    for (beat, value) in [(0.0, 0.5), (1.0, -0.5), (2.0, 0.5)] {
        run(&mut session, "automation.addPoint", json!({ "track": 1, "param": "pan", "beat": beat, "value": value }))
            .unwrap();
    }
    let before = pan_points(&session);
    let scale = |continued: bool| json!({ "track": 1, "param": "pan", "start": 0, "end": 2, "factor": 1.2, "continue": continued });
    run(&mut session, "automation.scaleRange", scale(false)).unwrap();
    run(&mut session, "automation.scaleRange", scale(true)).unwrap();
    run(&mut session, "automation.scaleRange", scale(true)).unwrap();
    assert_ne!(pan_points(&session), before);
    run(&mut session, "edit.undo", json!({})).unwrap();
    assert_eq!(pan_points(&session), before);
}

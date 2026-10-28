// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use suhina_model::{Device, Extension};

#[test]
fn the_copy_removes_local_metadata_and_input_bindings_without_changing_the_source() {
    let mut source = crate::starter_project();
    source.extensions.0.push(Extension {
        format: "private".into(),
        key: "editor".into(),
        data: b"/private/project/secret.wav".to_vec(),
    });
    source.tracks[0].midi_input.port = MidiPort::Named("My local keyboard".into());
    let before = source.clone();
    let (copy, bytes) = encode(&source).unwrap();
    assert_eq!(source, before);
    assert!(copy.extensions.0.is_empty());
    assert_eq!(copy.tracks[0].midi_input, input());
    assert_eq!(copy.tracks[0].audio_input, audio_input());
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(!text.contains("secret.wav"));
    assert!(!text.contains("My local keyboard"));
    assert_eq!(decode(&bytes).unwrap(), copy);
}

#[test]
fn audio_external_devices_opaque_state_and_excessive_sources_are_refused() {
    let mut source = crate::starter_project();
    source.tracks[0].content.audio = true;
    assert!(encode(&source).is_err());
    source.tracks[0].content.audio = false;
    for kind in [
        DeviceKind::Clap { id: "/private/plugin".into() },
        DeviceKind::Sd,
        DeviceKind::Builtin("sampler".into()),
        DeviceKind::Builtin("nam-amp-modeler".into()),
    ] {
        source.channels[0].devices = vec![Device {
            name: "Device".into(),
            kind,
            enabled: false,
            state: None,
            params: vec![],
            automation: vec![],
            extensions: Extensions::default(),
        }];
        assert!(encode(&source).is_err());
    }
    source.channels[0].devices[0].kind = DeviceKind::Builtin("fm-synth".into());
    source.channels[0].devices[0].state = Some(vec![0; 4]);
    assert!(encode(&source).is_err());
    source.channels[0].devices.clear();
    let first = source.tracks[0].clone();
    source.tracks = vec![first; 65];
    assert!(encode(&source).is_err());
}

#[test]
fn the_remote_document_requires_canonical_bytes_and_the_file_free_subset() {
    let (_, bytes) = encode(&crate::starter_project()).unwrap();
    assert!(decode(&vec![b' '; MAX_DOCUMENT + 1]).is_err());
    for altered in [
        String::from_utf8(bytes.clone()).unwrap() + " ",
        String::from_utf8(bytes.clone()).unwrap().replace("\"version\": 4", "\"version\": 4, \"version\": 4"),
        String::from_utf8(bytes.clone()).unwrap().replace("\"version\": 4", "\"version\": 99"),
        String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"project\": {",
            "\"project\": {\"file\":\"/private/audio.wav\",",
            1,
        ),
        String::from_utf8(bytes.clone()).unwrap().replacen("\"project\": {", "\"project\": {\"unknown\":true,", 1),
    ] {
        assert!(decode(altered.as_bytes()).is_err());
    }
    let mut tree: Value = serde_json::from_slice(&bytes).unwrap();
    tree["project"]["tracks"] = Value::Array(vec![tree["project"]["tracks"][0].clone(); 65]);
    assert!(decode(serde_json::to_string(&tree).unwrap().as_bytes()).is_err());
}

#[test]
fn authored_v3_automation_migrates_to_v4_before_canonical_sharing_without_changing_the_input() {
    use suhina_model::{AutomationLane, AutomationPoint};
    let mut p = crate::starter_project();
    p.channels[0].automation.push(AutomationLane {
        param: "volume".into(),
        enabled: true,
        points: vec![AutomationPoint::new(0., 0.1), AutomationPoint::new(4., 0.8)],
    });
    let mut old: Value = serde_json::from_str(&suhina_project_file::to_json(&p)).unwrap();
    old["version"] = serde_json::json!(3);
    old["project"]["channels"][0]["automation"][0]["points"][0]["curve"] = serde_json::json!(-0.5);
    let input = serde_json::to_string(&old).unwrap();
    let before = input.clone();
    let migrated = suhina_project_file::from_json(&input).unwrap();
    assert!(!migrated.channels[0].automation[0].points[0].curve.is_straight());
    let (copy, bytes) = encode(&migrated).unwrap();
    assert_eq!(input, before);
    assert_eq!(decode(&bytes).unwrap(), copy);
    assert!(decode(input.as_bytes()).is_err(), "portable copies require the current exact version");
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["version"], 4);
    assert_eq!(migrated.channels[0].automation, copy.channels[0].automation);
    assert_eq!(
        suhina_engine::render_offline(&migrated, 8000, 512).unwrap(),
        suhina_engine::render_offline(&copy, 8000, 512).unwrap()
    );
}

#[test]
fn v4_bezier_controls_survive_sharing_and_invalid_or_noncanonical_controls_are_refused() {
    use suhina_model::{AutomationLane, AutomationPoint, automation::Curve};
    let mut p = crate::starter_project();
    let mut point = AutomationPoint::new(0., 0.1);
    point.curve = Curve::new(0.1, 0.9, 0.8, 0.2).unwrap();
    p.channels[0].automation.push(AutomationLane {
        param: "volume".into(),
        enabled: true,
        points: vec![point, AutomationPoint::new(4., 0.8)],
    });
    let (copy, bytes) = encode(&p).unwrap();
    assert_eq!(copy.channels[0].automation, p.channels[0].automation);
    for bad in [
        serde_json::json!([0, 0, 2, 1]),
        serde_json::json!([0, 0, 1]),
        serde_json::json!(0.5),
        serde_json::json!(["x", 0, 1, 1]),
    ] {
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["project"]["channels"][0]["automation"][0]["points"][0]["curve"] = bad;
        assert!(decode(&serde_json::to_vec_pretty(&value).unwrap()).is_err());
    }
}

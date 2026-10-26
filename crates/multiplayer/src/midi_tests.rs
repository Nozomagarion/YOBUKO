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
        String::from_utf8(bytes.clone()).unwrap().replace("\"version\": 3", "\"version\": 3, \"version\": 3"),
        String::from_utf8(bytes.clone()).unwrap().replace("\"version\": 3", "\"version\": 99"),
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

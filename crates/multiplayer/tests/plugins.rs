// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used)] // Assertions in test-only helpers.

//! Projects with plug-ins and built-in effects, shared online and on the LAN: the settings
//! travel with the copy, and built-in parameters are edited together.

use serde_json::{Value, json};
use std::time::{Duration, Instant};
use suhina_engine::AudioPool;
use suhina_model::{Device, DeviceKind, DeviceParam};
use suhina_multiplayer::{ONLINE_LOOPBACK, Service};

fn wait(service: &mut Service, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let value = service.execute("multiplayer.state", &json!({})).unwrap();
        assert!(value["mode"] != "error", "service failed: {}", value["error"]);
        if ready(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "state timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn plug_in_settings_travel_and_built_in_effects_are_edited_together() {
    for address in [ONLINE_LOOPBACK, "127.0.0.1:0"] {
        let dirs = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut project = suhina_multiplayer::starter_project();
        let mut synth = Device::new("Serum", DeviceKind::Vst3 { id: "0123456789abcdef0123456789abcdef".into() });
        // Larger than one transfer chunk and than the old 2 MiB document.
        synth.state = Some((0..3_000_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect());
        synth.params = vec![DeviceParam::new("7", 0.5)];
        project.channels[0].devices = vec![synth, Device::new("Reverb", DeviceKind::Builtin("reverb".into()))];
        let mut host = Service::default();
        host.set_storage_root(Some(dirs.0.path().into())).unwrap();
        host.select_audio_project(&project, &AudioPool::new()).unwrap();
        host.execute("multiplayer.hostProject", &json!({"address":address})).unwrap();
        let ready = wait(&mut host, |s| s["mode"] == "hosting");
        let shared = host.take_shared().unwrap();
        let local = shared.session().unwrap().project().clone();
        shared.keep();
        let mut guest = Service::default();
        guest.set_storage_root(Some(dirs.1.path().into())).unwrap();
        guest.execute("multiplayer.join", &json!({"invitation":ready["invitation"]})).unwrap();
        wait(&mut guest, |s| s["mode"] == "connected");
        let snapshot = guest.take_shared().unwrap();
        let remote = snapshot.session().unwrap().project().clone();
        snapshot.keep();
        assert_eq!(remote, local, "{address}");
        assert_eq!(remote.channels[0].devices[0].state, project.channels[0].devices[0].state, "{address}");
        // The guest turns a Reverb knob; the host's copy follows.
        let track = remote.tracks.iter().find(|t| t.channel == Some(remote.channels[0].id)).unwrap().id.0;
        let info = suhina_device_registry::info("reverb").unwrap();
        let spec = &info.params[0];
        let value = spec.clamp(spec.max);
        let edit = json!({"command":"device.setSharedParam","track":track,"device":1,"param":spec.id,"value":value});
        guest.execute("multiplayer.edit", &json!({"base_revision":0,"edit":edit.to_string()})).unwrap();
        wait(&mut host, |s| s["controls"]["revision"] == 1);
        // A plug-in's own parameters are not edited remotely.
        let edit = json!({"command":"device.setSharedParam","track":track,"device":0,"param":"7","value":0.25});
        guest.execute("multiplayer.edit", &json!({"base_revision":1,"edit":edit.to_string()})).unwrap();
        let refused = wait(&mut guest, |s| s["error"].as_str().is_some());
        assert_eq!(refused["controls"]["revision"], 1, "{address}");
        host.execute("multiplayer.stop", &json!({})).unwrap();
    }
}

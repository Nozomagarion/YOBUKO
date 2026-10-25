// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! No socket, window, sound card or files: exercise real renderers on two local session replicas.

use serde_json::json;
use suhina_collaboration::{Edit, Host, Replica, Request, Role};
use suhina_engine::{AudioPool, Renderer};
use suhina_session::{Player, blank_project};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = Host::new(blank_project())?;
    let guest = host.add_peer(Role::Editor)?;
    let mut a = Replica::new(host.snapshot());
    let mut b = Replica::new(host.snapshot());
    let mut render_a = Renderer::stopped(a.project(), &AudioPool::default(), 8000)?;
    let mut render_b = Renderer::stopped(b.project(), &AudioPool::default(), 8000)?;
    let monitor_a = render_a.monitor();
    let monitor_b = render_b.monitor();
    a.attach_player(Player::new(render_a.controller(), None));
    b.attach_player(Player::new(render_b.controller(), None));
    a.audition("transport.setPosition", &json!({"beats": 4}))?;
    b.audition("transport.setPosition", &json!({"beats": 68}))?;
    a.audition("transport.play", &json!({}))?;
    b.audition("transport.play", &json!({}))?;
    let ack = host.submit(
        guest,
        Request {
            sequence: 1,
            base_revision: 0,
            edit: Edit::new("track.rename", json!({"track": 1, "name": "Shared melody"}))?,
        },
    )?;
    a.apply(&ack)?;
    b.apply(&ack)?;
    let mut buffer = [0.0; 128];
    render_a.process(&mut buffer);
    render_b.process(&mut buffer);
    if a.project() != b.project() || a.project() != host.project() {
        return Err(std::io::Error::other("the project replicas did not converge").into());
    }
    println!("Shared project confirmed at revision {}.", host.revision());
    println!("Independent playheads: A = {:.3} beats; B = {:.3} beats.", monitor_a.beat(), monitor_b.beat());
    a.audition("transport.stop", &json!({}))?;
    render_a.process(&mut buffer);
    render_b.process(&mut buffer);
    println!("After A stops: A playing = {}; B playing = {}.", monitor_a.running(), monitor_b.running());
    println!("Offline experiment only: no network connection or desktop multiplayer UI yet.");
    Ok(())
}

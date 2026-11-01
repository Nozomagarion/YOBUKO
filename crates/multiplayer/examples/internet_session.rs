// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::print_stdout, clippy::print_stderr)] // A command-line probe reports on the terminal.

//! A real Internet session between separate computers, without a window or a sound card.
//!
//! `internet_session host --guests N --code-file PATH` hosts over the Internet, writes the
//! code a friend would type into PATH, waits for N guests, renames track 1 and waits until every
//! guest's own edit has arrived. `internet_session join --code-file PATH --index K --guests N`
//! reads that code, joins, waits for the host's edit, renames its own track and waits until it
//! sees everyone's edits. Each side exits 0 only when the whole exchange went through, so a
//! release check can run the host and its guests on different machines and networks.

use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use suhina_multiplayer::{ONLINE, ONLINE_LOOPBACK, Service};

type Result<T> = std::result::Result<T, String>;

struct Options {
    role: String,
    code_file: PathBuf,
    guests: usize,
    index: usize,
    timeout: Duration,
    linger: Duration,
    long_code: bool,
    address: &'static str,
}

fn options() -> Result<Options> {
    let mut args = std::env::args().skip(1);
    let role = args.next().ok_or("usage: internet_session host|join --code-file PATH [...]")?;
    let mut o = Options {
        role,
        code_file: PathBuf::new(),
        guests: 1,
        index: 1,
        timeout: Duration::from_secs(600),
        linger: Duration::from_secs(30),
        long_code: false,
        address: ONLINE,
    };
    while let Some(flag) = args.next() {
        if flag == "--long-code" {
            o.long_code = true;
            continue;
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = || value.parse::<u64>().map_err(|_| format!("{flag} needs a number"));
        match flag.as_str() {
            "--code-file" => o.code_file = PathBuf::from(&value),
            "--guests" => o.guests = number()? as usize,
            "--index" => o.index = number()? as usize,
            "--timeout" => o.timeout = Duration::from_secs(number()?),
            "--linger" => o.linger = Duration::from_secs(number()?),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    if o.code_file.as_os_str().is_empty() {
        return Err("--code-file is required".into());
    }
    Ok(o)
}

fn state(service: &mut Service) -> Result<Value> {
    service.execute("multiplayer.state", &json!({})).map_err(|e| format!("state failed: {e:?}"))
}

/// Polls the session until `ready` holds, failing on a session error or the deadline.
fn wait(service: &mut Service, what: &str, deadline: Instant, ready: impl Fn(&Value) -> bool) -> Result<Value> {
    let started = Instant::now();
    let mut last_report = Instant::now();
    loop {
        let value = state(service)?;
        if value["mode"] == "error" || value["mode"] == "disconnected" {
            return Err(format!("{what}: session {} ({})", value["mode"], value["error"]));
        }
        if ready(&value) {
            println!("[{:>6.1}s] {what}", started.elapsed().as_secs_f64());
            return Ok(value);
        }
        if Instant::now() > deadline {
            return Err(format!("{what}: timed out; last state mode={} error={}", value["mode"], value["error"]));
        }
        if last_report.elapsed() > Duration::from_secs(15) {
            println!(
                "  ... still waiting for {what} (mode={}, guests={}, revision={})",
                value["mode"], value["guests_connected"], value["controls"]["revision"]
            );
            last_report = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn names(value: &Value) -> Vec<String> {
    value["controls"]["tracks"]
        .as_array()
        .map(|tracks| tracks.iter().map(|t| t["name"].as_str().unwrap_or_default().to_owned()).collect())
        .unwrap_or_default()
}

/// Track (1-based) and name each participant writes: the host takes track 1, guest K the next ones.
fn mark(index: usize, tracks: usize) -> (usize, String) {
    let name = if index == 0 { "from-host".to_owned() } else { format!("from-guest-{index}") };
    (index % tracks.max(1) + 1, name)
}

fn everyone_seen(value: &Value, guests: usize) -> bool {
    let names = names(value);
    (0..=guests).all(|i| {
        let (track, name) = mark(i, names.len());
        names.get(track - 1).is_some_and(|n| *n == name)
    })
}

/// Sends this participant's rename until the host confirms it, retrying when another edit won.
fn write_mark(service: &mut Service, index: usize, deadline: Instant) -> Result<()> {
    loop {
        let value = state(service)?;
        let names = names(&value);
        let (track, name) = mark(index, names.len());
        if names.get(track - 1).is_some_and(|n| *n == name) {
            println!("edit confirmed by the host: track {track} is now {name:?}");
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("our edit never reached the host".into());
        }
        let base = value["controls"]["revision"].as_u64().unwrap_or(0);
        let edit = json!({"command":"track.rename","track":track,"name":name});
        service
            .execute("multiplayer.edit", &json!({"base_revision":base,"edit":edit.to_string()}))
            .map_err(|e| format!("edit failed: {e:?}"))?;
        let sent = Instant::now();
        while sent.elapsed() < Duration::from_secs(3) {
            let now = state(service)?;
            if names_of(&now, track) == name || now["controls"]["revision"].as_u64() != Some(base) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn names_of(value: &Value, track: usize) -> String {
    names(value).get(track - 1).cloned().unwrap_or_default()
}

fn host(o: &Options) -> Result<()> {
    let deadline = Instant::now() + o.timeout;
    let mut service = Service::default();
    service.execute("multiplayer.host", &json!({"address":o.address})).map_err(|e| format!("could not host: {e:?}"))?;
    wait(&mut service, "hosting", deadline, |v| v["mode"] == "hosting")?;
    // The short code appears once its lobby is published, a few seconds after the session opens.
    let shown = (Instant::now() + Duration::from_secs(90)).min(deadline);
    let ready = wait(&mut service, "short code shown", shown, |v| o.long_code || v["short_code"].is_string())?;
    let short = ready["short_code"].as_str().unwrap_or_default().to_owned();
    let long = ready["invitation"].as_str().unwrap_or_default().to_owned();
    let code = if o.long_code || short.is_empty() { long } else { short.clone() };
    println!("short code: {short:?} (max guests {})", ready["max_guests"]);
    std::fs::write(&o.code_file, &code).map_err(|e| format!("could not write the code: {e}"))?;
    wait(&mut service, &format!("{} guest(s) connected", o.guests), deadline, |v| {
        v["guests_connected"].as_u64().is_some_and(|n| n as usize >= o.guests)
    })?;
    write_mark(&mut service, 0, deadline)?;
    let done = wait(&mut service, "every guest's edit arrived", deadline, |v| everyone_seen(v, o.guests))?;
    println!("tracks: {:?}", names(&done));
    // Leave the guests time to see the final state before the host ends the session.
    let end = Instant::now() + o.linger;
    while Instant::now() < end {
        let _ = state(&mut service)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    service.execute("multiplayer.stop", &json!({})).map_err(|e| format!("stop failed: {e:?}"))?;
    println!("HOST OK");
    Ok(())
}

fn join(o: &Options) -> Result<()> {
    let deadline = Instant::now() + o.timeout;
    let code = std::fs::read_to_string(&o.code_file).map_err(|e| format!("could not read the code: {e}"))?;
    let code = code.trim();
    println!("joining guest {} with a {}-character code", o.index, code.len());
    let mut service = Service::default();
    let started = Instant::now();
    service.execute("multiplayer.join", &json!({"invitation":code})).map_err(|e| format!("could not join: {e:?}"))?;
    wait(&mut service, "connected to the host", deadline, |v| v["mode"] == "connected")?;
    println!("connection took {:.1}s", started.elapsed().as_secs_f64());
    wait(&mut service, "host's edit received", deadline, |v| {
        let names = names(v);
        let (track, name) = mark(0, names.len());
        names.get(track - 1).is_some_and(|n| *n == name)
    })?;
    write_mark(&mut service, o.index, deadline)?;
    let done = wait(&mut service, "everyone's edits received", deadline, |v| everyone_seen(v, o.guests))?;
    println!("tracks: {:?}", names(&done));
    service.execute("multiplayer.stop", &json!({})).map_err(|e| format!("stop failed: {e:?}"))?;
    println!("GUEST {} OK", o.index);
    Ok(())
}

/// Host and guests in one process on this computer only: checks this probe without the Internet.
fn selftest(o: &Options) -> Result<()> {
    let host_options = Options { address: ONLINE_LOOPBACK, linger: Duration::from_secs(3), ..clone(o) };
    let host = std::thread::spawn(move || host(&host_options));
    let started = Instant::now();
    while !o.code_file.exists() || std::fs::metadata(&o.code_file).map(|m| m.len()).unwrap_or(0) == 0 {
        if started.elapsed() > Duration::from_secs(60) {
            return Err("the host never wrote its code".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let guests: Vec<_> = (1..=o.guests)
        .map(|index| {
            let guest = Options { index, ..clone(o) };
            std::thread::spawn(move || join(&guest))
        })
        .collect();
    for guest in guests {
        guest.join().map_err(|_| "a guest panicked")??;
    }
    host.join().map_err(|_| "the host panicked")?
}

fn clone(o: &Options) -> Options {
    Options { role: o.role.clone(), code_file: o.code_file.clone(), ..*o }
}

fn main() {
    let result = options().and_then(|o| match o.role.as_str() {
        "selftest" => selftest(&o),
        "host" => host(&o),
        "join" => join(&o),
        other => Err(format!("unknown role {other}; use host or join")),
    });
    if let Err(error) = result {
        eprintln!("FAILED: {error}");
        std::process::exit(1);
    }
}

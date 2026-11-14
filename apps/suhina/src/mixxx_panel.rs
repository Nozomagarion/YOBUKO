// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO: Mixxx inside this window. The DJ space (`suhina_ui::app::mixxx_space`) leaves a
//! rectangle for Mixxx; this starts Mixxx, set up for YOBUKO (`App::mixxx_launch`), and the
//! plug-in host with `--adopt` (`crates/plugin-host/src/adopt.rs`), which takes Mixxx's main
//! window, without its title bar and borders, into this window and keeps it over that
//! rectangle. Every one of Mixxx's decks, buttons, menus and settings is there.
//!
//! Windows and Linux (X11) only: macOS cannot show another program's window inside this one,
//! and neither can a Wayland session, so there the DJ space shows YOBUKO's own decks.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use suhina_ui::app::mixxx_space::{MixxxAsk, MixxxHost, MixxxLaunch, MixxxStatus, MixxxView};

/// How long Mixxx's window may take to appear before YOBUKO gives up on it.
const START_LIMIT: Duration = Duration::from_secs(90);
/// How often the page looks at Mixxx while it starts.
const STARTING_TICK: Duration = Duration::from_millis(250);
/// How long quitting YOBUKO waits for Mixxx to close by itself.
const CLOSE_LIMIT: Duration = Duration::from_secs(5);

/// The plug-in host holding Mixxx's window.
struct Holder {
    child: Child,
    input: ChildStdin,
    heard: mpsc::Receiver<String>,
}

/// Mixxx in the DJ space.
#[derive(Default)]
pub struct MixxxPanel {
    mixxx: Option<Child>,
    holder: Option<Holder>,
    status: MixxxStatus,
    started: Option<Instant>,
    /// The last line sent to the holder, so a place is sent only when it changes.
    sent: Option<String>,
}

fn quiet(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        command.creation_flags(0x0800_0000);
    }
    command
}

impl MixxxPanel {
    /// One frame: starts Mixxx the first time the DJ space shows it, puts its window over
    /// `view`, and says how far it is.
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        owner: Option<u64>,
        shown: bool,
        view: Option<MixxxView>,
        asks: Vec<MixxxAsk>,
        launch: impl FnOnce() -> Option<MixxxLaunch>,
    ) {
        for ask in asks {
            match ask {
                MixxxAsk::Restart => {
                    self.stop();
                    self.status = MixxxStatus::Off;
                }
            }
        }
        if shown && self.status == MixxxStatus::Off {
            self.status = match owner {
                None => MixxxStatus::Unsupported(unsupported().into()),
                Some(owner) => match launch() {
                    None => MixxxStatus::Missing,
                    Some(launch) => self.start(owner, launch),
                },
            };
        }
        self.listen();
        if self.status == MixxxStatus::Starting {
            if self.started.is_some_and(|at| at.elapsed() > START_LIMIT) {
                self.stop();
                self.status = MixxxStatus::Stopped("its window did not appear.".into());
            } else {
                ctx.request_repaint_after(STARTING_TICK);
            }
        }
        let line = match view.filter(|_| shown) {
            Some(view) if !view.covered => {
                let scale = ctx.pixels_per_point();
                let px = |v: f32| (v * scale).round() as i64;
                let rect = view.rect;
                format!(
                    "place {} {} {} {}",
                    px(rect.min.x),
                    px(rect.min.y),
                    px(rect.width()).max(0),
                    px(rect.height()).max(0)
                )
            }
            _ => "hide".to_owned(),
        };
        self.send(line);
    }

    /// What the DJ space shows of Mixxx.
    pub fn host(&self) -> MixxxHost {
        MixxxHost { status: self.status.clone() }
    }

    fn start(&mut self, owner: u64, launch: MixxxLaunch) -> MixxxStatus {
        let mut command = Command::new(&launch.program);
        command.args(&launch.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(dir) = launch.program.parent() {
            command.current_dir(dir);
        }
        // Mixxx's window must be an X11 one to go inside this one, also in a Wayland session.
        if cfg!(target_os = "linux") {
            command.env("QT_QPA_PLATFORM", "xcb");
        }
        let mixxx = match quiet(&mut command).spawn() {
            Ok(child) => child,
            Err(e) => return MixxxStatus::Stopped(format!("Mixxx could not start ({e}).")),
        };
        let host = suhina_plugins::library::host_next_to_this_program();
        let mut command = Command::new(host);
        command
            .arg("--adopt")
            .arg(owner.to_string())
            .arg(mixxx.id().to_string())
            // Its main window's title has its name; a dialog it shows first is not taken.
            .arg("Mixxx")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = match quiet(&mut command).spawn() {
            Ok(child) => child,
            Err(e) => {
                self.mixxx = Some(mixxx);
                self.stop();
                return MixxxStatus::Stopped(format!("YOBUKO could not take Mixxx's window in ({e})."));
            }
        };
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            self.mixxx = Some(mixxx);
            self.stop();
            return MixxxStatus::Stopped("YOBUKO could not take Mixxx's window in.".into());
        };
        let (tell, heard) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if tell.send(line).is_err() {
                    break;
                }
            }
        });
        self.mixxx = Some(mixxx);
        self.holder = Some(Holder { child, input, heard });
        self.started = Some(Instant::now());
        self.sent = None;
        MixxxStatus::Starting
    }

    /// Takes in what the holder said, and notices Mixxx closing.
    fn listen(&mut self) {
        let mut said = Vec::new();
        if let Some(holder) = &self.holder {
            said.extend(holder.heard.try_iter());
        }
        for line in said {
            if line == "adopted" {
                self.status = MixxxStatus::Ready;
            } else if line == "gone" {
                self.stop();
                self.status = MixxxStatus::Stopped("Mixxx closed. Start it again here.".into());
            } else if let Some(why) = line.strip_prefix("error ") {
                self.stop();
                self.status = MixxxStatus::Stopped(format!("YOBUKO could not take Mixxx's window in: {why}."));
            }
        }
        let closed = self.mixxx.as_mut().is_some_and(|m| m.try_wait().ok().flatten().is_some());
        if closed {
            self.stop();
            self.status = MixxxStatus::Stopped("Mixxx closed. Start it again here.".into());
        }
    }

    fn send(&mut self, line: String) {
        if self.sent.as_deref() == Some(line.as_str()) {
            return;
        }
        let Some(holder) = &mut self.holder else { return };
        if writeln!(holder.input, "{line}").and_then(|()| holder.input.flush()).is_ok() {
            self.sent = Some(line);
        }
    }

    /// Lets Mixxx and its holder go at once.
    fn stop(&mut self) {
        self.close(Duration::ZERO);
    }

    /// Lets Mixxx and its holder go: Mixxx is asked to quit, as its own close button would,
    /// and stopped if it has not after `wait`.
    fn close(&mut self, wait: Duration) {
        if let Some(mut holder) = self.holder.take() {
            let _ = writeln!(holder.input, "close").and_then(|()| holder.input.flush());
            drop(holder.input);
            if let Some(mixxx) = &mut self.mixxx {
                let until = Instant::now() + wait;
                while Instant::now() < until && mixxx.try_wait().ok().flatten().is_none() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            let _ = holder.child.kill();
            let _ = holder.child.wait();
        }
        if let Some(mut mixxx) = self.mixxx.take()
            && mixxx.try_wait().ok().flatten().is_none()
        {
            let _ = mixxx.kill();
            let _ = mixxx.wait();
        }
        self.started = None;
        self.sent = None;
    }

    /// YOBUKO quits: Mixxx is asked to close and given a moment to save what it keeps.
    pub fn quit(&mut self) {
        self.close(CLOSE_LIMIT);
    }
}

fn unsupported() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS cannot show Mixxx inside YOBUKO's window yet."
    } else {
        "this session cannot show Mixxx inside YOBUKO's window (it needs an X11 desktop)."
    }
}

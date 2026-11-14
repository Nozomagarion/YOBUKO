// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO: `--adopt <window> <pid> [<title word>]` shows another program's main window inside the app's
//! window, the way an embedded plug-in editor shows in its device panel (ADR 0006 › 10). The
//! DJ space uses it for Mixxx: YOBUKO starts Mixxx, then this, which waits for Mixxx's main
//! window (the one whose title has the word, when one is given: a dialog shown before it is
//! not taken), takes its title bar and borders away and puts it in an embedded window of its own
//! (`window::Window::open_embedded`: the clip over YOBUKO's window and the view inside it).
//!
//! YOBUKO says where it goes, one line on standard input at a time:
//!
//! - `place X Y WIDTH HEIGHT`: the rectangle, in pixels of YOBUKO's window's inside;
//! - `hide`: out of sight (another space shows);
//! - `close`: asks the program to quit, as its own close button would.
//!
//! This answers on standard output: `adopted` once the window is in, `gone` once the program's
//! window has closed (this then exits). When YOBUKO goes away (standard input ends), the
//! program is asked to quit too.
//!
//! Windows (Win32) and Linux (X11) only: macOS cannot show another process's window inside
//! one of its own, so there `--adopt` fails at once.

use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

use suhina_plugins::Placement;

use crate::window::{self, Mode, Size, Window};

#[cfg(windows)]
#[path = "adopt/windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "adopt/x11.rs"]
mod imp;

/// What YOBUKO asked, one line of standard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    Place { x: i32, y: i32, width: u32, height: u32 },
    Hide,
    Close,
}

impl Ask {
    /// A line of the protocol above; anything else is `None`.
    pub(crate) fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        let ask = match words.next()? {
            "place" => {
                let mut number = || words.next().and_then(|w| w.parse::<i64>().ok());
                let (x, y, width, height) = (number()?, number()?, number()?, number()?);
                Self::Place {
                    x: i32::try_from(x).ok()?,
                    y: i32::try_from(y).ok()?,
                    width: u32::try_from(width).ok()?,
                    height: u32::try_from(height).ok()?,
                }
            }
            "hide" => Self::Hide,
            "close" => Self::Close,
            _ => return None,
        };
        words.next().is_none().then_some(ask)
    }

    /// Where the embedded window goes for this ask: the clip and the view at the same place,
    /// as the whole program shows, never scrolled.
    pub(crate) fn placement(self) -> Option<Placement> {
        match self {
            Self::Place { x, y, width, height } if width > 0 && height > 0 => {
                Some(Placement { x, y, clip_x: x, clip_y: y, clip_width: width, clip_height: height })
            }
            Self::Place { .. } | Self::Hide => Some(Placement::HIDDEN),
            Self::Close => None,
        }
    }
}

/// How long the loop waits for something to do before looking for the program's window again.
const TICK: Duration = Duration::from_millis(30);

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// `--adopt <window> <pid> [<title word>]`: runs until the program's window is gone. Exits the
/// process.
pub(crate) fn main(parent: &str, pid: Option<&String>, title: Option<&String>) -> ! {
    let code = match run(parent, pid.map(String::as_str), title.map(String::as_str)) {
        Ok(()) => 0,
        Err(e) => {
            say(&format!("error {e}"));
            eprintln!("suhina-plugin-host: {e}");
            1
        }
    };
    std::process::exit(code);
}

#[cfg(not(any(windows, target_os = "linux")))]
fn run(_parent: &str, _pid: Option<&str>, _title: Option<&str>) -> Result<(), String> {
    Err("this system cannot show another program's window inside YOBUKO's".into())
}

#[cfg(any(windows, target_os = "linux"))]
fn run(parent: &str, pid: Option<&str>, title: Option<&str>) -> Result<(), String> {
    let parent: u64 = parent.parse().map_err(|_| "--adopt needs YOBUKO's window")?;
    let pid: u32 = pid.and_then(|p| p.parse().ok()).ok_or("--adopt needs the program's process id")?;
    let view = Window::open_as(Mode::Embedded { parent }, "", Size { width: 1, height: 1 }, false)?;
    let asks = read_asks();
    let mut program: Option<imp::Program> = None;
    let mut shown: Option<Size> = None;
    let mut closing = false;
    loop {
        tick();
        window::pump();
        let mut taken = Vec::new();
        loop {
            match asks.try_recv() {
                Ok(ask) => taken.push(ask),
                Err(mpsc::TryRecvError::Empty) => break,
                // YOBUKO is gone: the program goes too.
                Err(mpsc::TryRecvError::Disconnected) => {
                    taken.push(Ask::Close);
                    break;
                }
            }
        }
        for ask in taken {
            if let Some(placement) = ask.placement() {
                shown = (placement.clip_width > 0)
                    .then_some(Size { width: placement.clip_width, height: placement.clip_height });
                if let Some(size) = shown {
                    window::resize(view.id(), size);
                }
                window::place(view.id(), placement);
            } else if !closing {
                closing = true;
                if let Some(program) = &program {
                    program.close();
                }
            }
            if closing && program.is_none() {
                // Nothing of the program's was in yet: there is nothing more to do here.
                return Ok(());
            }
        }
        match &program {
            None => {
                if let Some(mut found) = imp::Program::find(pid, title) {
                    let size = shown.unwrap_or(Size { width: 1, height: 1 });
                    found.adopt(&view, size)?;
                    program = Some(found);
                    say("adopted");
                }
            }
            Some(adopted) => {
                if !adopted.alive() {
                    say("gone");
                    return Ok(());
                }
                if let Some(size) = shown {
                    adopted.fit(size);
                }
                adopted.serve();
            }
        }
    }
}

/// Standard input's asks, read on a thread of their own. The receiver hangs up when input ends.
#[cfg(any(windows, target_os = "linux"))]
fn read_asks() -> mpsc::Receiver<Ask> {
    let (send, receive) = mpsc::channel();
    #[cfg(windows)]
    let waker = window::Waker::current();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(ask) = Ask::parse(&line)
                && send.send(ask).is_err()
            {
                break;
            }
            #[cfg(windows)]
            waker.wake();
        }
        drop(send);
        #[cfg(windows)]
        waker.wake();
    });
    receive
}

/// Waits a tick, or less when there is something to do.
#[cfg(windows)]
fn tick() {
    window::wait(TICK);
}

#[cfg(target_os = "linux")]
fn tick() {
    // The asks are taken right after; X events are polled by the program's connection.
    std::thread::sleep(TICK);
}

/// Whether a window titled `title` is the one asked for: any, or one whose title has `word`.
pub(crate) fn titled(title: &str, word: Option<&str>) -> bool {
    word.is_none_or(|word| title.to_lowercase().contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_are_read_from_their_lines() {
        assert_eq!(Ask::parse("place 10 -4 800 600"), Some(Ask::Place { x: 10, y: -4, width: 800, height: 600 }));
        assert_eq!(Ask::parse(" hide "), Some(Ask::Hide));
        assert_eq!(Ask::parse("close"), Some(Ask::Close));
        for wrong in ["", "place 1 2 3", "place 1 2 3 -4", "place a b c d", "hide now", "open", "place 1 2 3 4 5"] {
            assert_eq!(Ask::parse(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn the_title_word_keeps_dialogs_out() {
        assert!(titled("Mixxx 2.5.6", Some("Mixxx")));
        assert!(titled("Artist - Song | mixxx", Some("Mixxx")));
        assert!(!titled("Choose music library directory", Some("Mixxx")));
        assert!(titled("Anything", None));
    }

    #[test]
    fn a_place_shows_the_whole_window_and_an_empty_one_hides_it() {
        let place = Ask::Place { x: 5, y: 40, width: 300, height: 200 };
        assert_eq!(
            place.placement(),
            Some(Placement { x: 5, y: 40, clip_x: 5, clip_y: 40, clip_width: 300, clip_height: 200 })
        );
        assert_eq!(Ask::Place { x: 5, y: 40, width: 0, height: 200 }.placement(), Some(Placement::HIDDEN));
        assert_eq!(Ask::Hide.placement(), Some(Placement::HIDDEN));
        assert_eq!(Ask::Close.placement(), None);
    }
}

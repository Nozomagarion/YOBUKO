// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The connection to the rbxport program running hidden for the USB space: started here with
//! `YOBUKO_EMBED=<port>:<token>`, it connects back to this listener and the two exchange
//! frames, four bytes of length (big-endian) then that many bytes of JSON. Keep in step with
//! `third_party/rbxport-app/src-tauri/src/yobuko_embed.rs`.
//!
//! Answers to the webview's file requests (`asset`) are handed to the waiting responder on the
//! reading thread; everything else waits in `inbox` for the window's next frame, which the
//! reading thread asks for.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};

/// The largest frame accepted: a long track's waveform or a large picture fits many times over.
pub const MAX_FRAME: usize = 256 * 1024 * 1024;

/// How long rbxport may take to connect (a first start reads the whole library) before the
/// page says it did not start.
const CONNECT_WITHIN: Duration = Duration::from_secs(120);

/// The variable rbxport reads its connection from.
pub const ENV: &str = "YOBUKO_EMBED";

/// Writes one frame.
pub fn write_frame(stream: &mut impl Write, message: &Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    let length = u32::try_from(bytes.len()).map_err(std::io::Error::other)?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}

/// Reads one frame; `None` at the end of the connection.
pub fn read_frame(stream: &mut impl Read) -> std::io::Result<Option<Value>> {
    let mut length = [0u8; 4];
    match stream.read_exact(&mut length) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let length = usize::try_from(u32::from_be_bytes(length)).map_err(std::io::Error::other)?;
    if length > MAX_FRAME {
        return Err(std::io::Error::other("frame too large"));
    }
    let mut bytes = vec![0u8; length];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map(Some).map_err(std::io::Error::other)
}

/// A token nobody else on this computer can guess, for the one connection.
pub fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for round in 0..2u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(round);
        hasher.write_u128(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        );
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

/// Where the connection is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// rbxport is starting and has not connected yet.
    Waiting,
    Connected,
    /// It ended: why.
    Closed(String),
}

/// A request of the webview for a file, waiting for rbxport's answer.
pub type Responder = Box<dyn FnOnce(u16, Vec<(String, String)>, Vec<u8>) + Send>;

/// What the webviews' protocol handlers need: the way out and where answers go.
#[derive(Clone)]
pub struct Requests {
    out: mpsc::Sender<Value>,
    waiting: Arc<Mutex<HashMap<u64, Responder>>>,
    next: Arc<AtomicU64>,
}

impl Requests {
    /// Asks rbxport for a file: `scheme` `app` (the interface's own files, by `path`) or `rbl`
    /// (the library's pictures and sounds, by `uri`).
    pub fn ask(&self, mut message: Value, respond: Responder) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.insert(id, respond);
        }
        message["t"] = json!("asset");
        message["id"] = json!(id);
        if self.out.send(message).is_err()
            && let Some(respond) = self.waiting.lock().ok().and_then(|mut w| w.remove(&id))
        {
            respond(503, Vec::new(), Vec::new());
        }
    }
}

/// The running rbxport and its connection.
pub struct Link {
    child: Option<Child>,
    out: mpsc::Sender<Value>,
    inbox: Arc<Mutex<VecDeque<Value>>>,
    state: Arc<Mutex<State>>,
    requests: Requests,
}

impl Link {
    /// Starts `program` hidden, connected to this window; `wake` is called whenever something
    /// arrives, so the window draws a frame to take it.
    pub fn start(program: &Path, wake: impl Fn() + Send + Sync + 'static) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = token();
        let mut command = Command::new(program);
        command.env(ENV, format!("{port}:{token}")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(dir) = program.parent() {
            command.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window.
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn()?;
        let (out, outgoing) = mpsc::channel::<Value>();
        let inbox = Arc::new(Mutex::new(VecDeque::new()));
        let state = Arc::new(Mutex::new(State::Waiting));
        let waiting: Arc<Mutex<HashMap<u64, Responder>>> = Arc::new(Mutex::new(HashMap::new()));
        let requests = Requests { out: out.clone(), waiting: waiting.clone(), next: Arc::new(AtomicU64::new(1)) };
        let wake = Arc::new(wake);
        {
            let (inbox, state, wake) = (inbox.clone(), state.clone(), wake.clone());
            std::thread::Builder::new().name("rbxport-link".into()).spawn(move || {
                let ended = match connect(&listener, &token) {
                    Ok(stream) => {
                        set(&state, State::Connected);
                        wake();
                        serve(stream, outgoing, &inbox, &waiting, &*wake)
                    }
                    Err(e) => e,
                };
                if let Ok(mut waiting) = waiting.lock() {
                    for (_, respond) in waiting.drain() {
                        respond(503, Vec::new(), Vec::new());
                    }
                }
                set(&state, State::Closed(ended));
                wake();
            })?;
        }
        Ok(Self { child: Some(child), out, inbox, state, requests })
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Closed("the connection broke".into()))
    }

    /// Sends a message to rbxport (queued until it is connected).
    pub fn send(&self, message: Value) {
        let _ = self.out.send(message);
    }

    /// What arrived since the last call.
    pub fn take(&self) -> Vec<Value> {
        self.inbox.lock().map(|mut inbox| inbox.drain(..).collect()).unwrap_or_default()
    }

    pub fn requests(&self) -> Requests {
        self.requests.clone()
    }
}

impl Drop for Link {
    /// rbxport ends with the connection; it is told first, and stopped if it does not listen.
    fn drop(&mut self) {
        let _ = self.out.send(json!({ "t": "quit" }));
        if let Some(mut child) = self.child.take() {
            let _ = std::thread::Builder::new().name("rbxport-end".into()).spawn(move || {
                for _ in 0..40 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

fn set(state: &Mutex<State>, new: State) {
    if let Ok(mut state) = state.lock() {
        *state = new;
    }
}

/// Waits for rbxport to connect and say the token.
fn connect(listener: &TcpListener, token: &str) -> Result<TcpStream, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let since = std::time::Instant::now();
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_nodelay(true);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let hello = read_frame(&mut stream).ok().flatten();
                let _ = stream.set_read_timeout(None);
                if hello.as_ref().and_then(|h| h.get("token")).and_then(Value::as_str) == Some(token) {
                    return Ok(stream);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if since.elapsed() > CONNECT_WITHIN {
                    return Err("rbxport did not start in time.".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("could not wait for rbxport: {e}")),
        }
    }
}

/// Writes what is sent and reads what comes until the connection ends; why it ended.
fn serve(
    stream: TcpStream,
    outgoing: mpsc::Receiver<Value>,
    inbox: &Mutex<VecDeque<Value>>,
    waiting: &Mutex<HashMap<u64, Responder>>,
    wake: &(dyn Fn() + Send + Sync),
) -> String {
    let Ok(mut writer) = stream.try_clone() else { return "the connection to rbxport broke.".into() };
    let _ = std::thread::Builder::new().name("rbxport-out".into()).spawn(move || {
        for message in outgoing {
            if write_frame(&mut writer, &message).is_err() {
                return;
            }
        }
    });
    let mut reader = stream;
    loop {
        match read_frame(&mut reader) {
            Ok(Some(message)) if message.get("t").and_then(Value::as_str) == Some("asset") => {
                let id = message.get("id").and_then(Value::as_u64).unwrap_or_default();
                if let Some(respond) = waiting.lock().ok().and_then(|mut w| w.remove(&id)) {
                    let (status, headers, body) = asset_answer(&message);
                    respond(status, headers, body);
                }
            }
            Ok(Some(message)) => {
                if let Ok(mut inbox) = inbox.lock() {
                    inbox.push_back(message);
                }
                wake();
            }
            Ok(None) => return "rbxport closed.".into(),
            Err(e) => return format!("the connection to rbxport broke: {e}"),
        }
    }
}

/// The status, headers and body of an `asset` answer.
pub fn asset_answer(message: &Value) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let status = message.get("status").and_then(Value::as_u64).and_then(|s| u16::try_from(s).ok()).unwrap_or(500);
    let headers = message
        .get("headers")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|pair| Some((pair.get(0)?.as_str()?.to_owned(), pair.get(1)?.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let body = message
        .get("body")
        .and_then(Value::as_str)
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        .unwrap_or_default();
    (status, headers, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_reads_back_as_it_was_written() {
        let mut bytes = Vec::new();
        let message = json!({ "t": "invoke", "id": 3, "cmd": "library_summary", "args": {} });
        write_frame(&mut bytes, &message).unwrap();
        let mut reader = std::io::Cursor::new(bytes);
        assert_eq!(read_frame(&mut reader).unwrap(), Some(message));
        assert_eq!(read_frame(&mut reader).unwrap(), None);
    }

    #[test]
    fn a_frame_too_large_is_refused_before_it_is_read() {
        let mut bytes = u32::MAX.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        assert!(read_frame(&mut std::io::Cursor::new(bytes)).is_err());
    }

    #[test]
    fn an_asset_answer_gives_its_status_headers_and_bytes() {
        let message = json!({
            "t": "asset", "id": 1, "status": 206,
            "headers": [["Content-Type", "audio/mpeg"], ["Content-Range", "bytes 0-2/10"]],
            "body": base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]),
        });
        let (status, headers, body) = asset_answer(&message);
        assert_eq!(status, 206);
        assert_eq!(headers[1], ("Content-Range".to_owned(), "bytes 0-2/10".to_owned()));
        assert_eq!(body, [1, 2, 3]);
        assert_eq!(asset_answer(&json!({})).0, 500);
    }

    #[test]
    fn two_tokens_differ() {
        assert_ne!(token(), token());
        assert_eq!(token().len(), 32);
    }

    /// A stand-in for rbxport: connects with the token, answers one command, then leaves.
    #[test]
    fn the_connection_carries_commands_and_answers_and_reports_its_end() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write_frame(&mut stream, &json!({ "t": "hello", "token": "secret" })).unwrap();
            let asked = read_frame(&mut stream).unwrap().unwrap();
            write_frame(&mut stream, &json!({ "t": "reply", "id": asked["id"], "ok": true, "value": 42 })).unwrap();
        });
        let stream = connect(&listener, "secret").unwrap();
        let (out, outgoing) = mpsc::channel();
        out.send(json!({ "t": "invoke", "id": 9 })).unwrap();
        let inbox = Mutex::new(VecDeque::new());
        let waiting = Mutex::new(HashMap::new());
        let ended = serve(stream, outgoing, &inbox, &waiting, &|| {});
        peer.join().unwrap();
        assert_eq!(ended, "rbxport closed.");
        let got: Vec<Value> = inbox.lock().unwrap().drain(..).collect();
        assert_eq!(got, [json!({ "t": "reply", "id": 9, "ok": true, "value": 42 })]);
    }
}

//! YOBUKO patch (not in upstream rbxport): rbxport drawn inside YOBUKO's own
//! window, with no window of its own on screen.
//!
//! YOBUKO starts this program with `YOBUKO_EMBED=<port>:<token>` and shows
//! rbxport's interface itself, in a webview inside its window (YOBUKO's
//! `apps/suhina/src/rbxport_panel.rs`). This process keeps everything else:
//! the library, the analysis, the players, the USB export, every command.
//!
//! - Every window rbxport opens (the main one, Preferences, the Sync Manager,
//!   Report bug, the Log) is still built, so that every command that names
//!   its window and every event sent to it works as before, but it stays
//!   hidden and loads `yobuko-bridge.html` instead of the interface. YOBUKO
//!   opens a page of the same name in its window instead.
//! - A command the page in YOBUKO runs comes here over a local TCP
//!   connection, is run by the bridge page of the same window with Tauri's
//!   own `invoke`, and its answer goes back the same way
//!   (`public/yobuko-bridge.js`). Events and callbacks follow the same road.
//! - The `rbl:` pictures and sounds and the interface's own files are served
//!   to YOBUKO's webview from here (`asset`).
//! - The menu bar is drawn by YOBUKO from [`menu_tree`]; a click comes back
//!   as the item's id and runs [`crate::menu::on_event`] as a click on the
//!   native menu does.
//!
//! The connection is one frame after the other each way: four bytes of
//! length (big-endian), then that many bytes of JSON. Keep in step with
//! YOBUKO's `apps/suhina/src/rbxport_panel/link.rs`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tauri::menu::MenuItemKind;
use tauri::{AppHandle, Manager, Runtime};

/// The variable YOBUKO starts this program with: `<port>:<token>`.
pub const ENV: &str = "YOBUKO_EMBED";

/// The page the hidden windows load in place of the interface.
pub const BRIDGE_PAGE: &str = "yobuko-bridge.html";

/// The largest frame either side accepts: a long track's waveform or a large
/// artwork fits many times over.
pub const MAX_FRAME: usize = 256 * 1024 * 1024;

/// How often the menu is looked at for changes (labels, greyed items).
const MENU_INTERVAL: Duration = Duration::from_millis(400);

/// The keys of the menu's items, as `menu.rs` gives them. Tauri cannot read
/// an item's accelerator back, and YOBUKO needs them both to show them and
/// to run the item when they are pressed in its webview.
pub const ACCELERATORS: &[(&str, &str)] = &[
    ("settings", "CmdOrCtrl+,"),
    ("import", "CmdOrCtrl+O"),
    ("import-folder", "CmdOrCtrl+Shift+O"),
    ("info", "CmdOrCtrl+I"),
    ("sub", "CmdOrCtrl+B"),
    ("fullscreen", "Shift+CmdOrCtrl+F"),
    ("layout-one", "CmdOrCtrl+7"),
    ("layout-two", "CmdOrCtrl+8"),
    ("layout-simple", "CmdOrCtrl+9"),
    ("layout-browser", "CmdOrCtrl+0"),
    ("undo", "CmdOrCtrl+Z"),
    ("redo", "CmdOrCtrl+Shift+Z"),
    ("locate-loaded", "CmdOrCtrl+L"),
];

struct Link {
    port: u16,
    token: String,
}

/// `<port>:<token>`, read once.
pub fn parse(value: &str) -> Option<(u16, String)> {
    let (port, token) = value.split_once(':')?;
    let port: u16 = port.trim().parse().ok().filter(|p| *p != 0)?;
    let token = token.trim();
    if token.is_empty() || token.len() > 256 {
        return None;
    }
    Some((port, token.to_owned()))
}

fn link() -> Option<&'static Link> {
    static LINK: OnceLock<Option<Link>> = OnceLock::new();
    LINK.get_or_init(|| {
        let value = std::env::var(ENV).ok()?;
        parse(&value).map(|(port, token)| Link { port, token })
    })
    .as_ref()
}

/// Whether this rbxport is drawn inside YOBUKO's window.
pub fn embedded() -> bool {
    link().is_some()
}

/// The page a new window loads: the interface, or the bridge when YOBUKO
/// draws the interface.
pub fn page() -> tauri::WebviewUrl {
    tauri::WebviewUrl::App(
        if embedded() {
            BRIDGE_PAGE
        } else {
            "index.html"
        }
        .into(),
    )
}

/// One hidden window: whether its bridge page answers yet, and what waits for it.
#[derive(Default)]
struct Bridge {
    ready: bool,
    queued: Vec<String>,
}

struct Embed {
    out: mpsc::Sender<Value>,
    bridges: Mutex<HashMap<String, Bridge>>,
}

static EMBED: OnceLock<Embed> = OnceLock::new();

fn send(message: Value) {
    if let Some(embed) = EMBED.get() {
        let _ = embed.out.send(message);
    }
}

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
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(std::io::Error::other)
}

/// Connects to YOBUKO, when it started this program, and serves it until it
/// goes away; then this program ends too, since nothing of it is on screen.
pub fn start(app: &AppHandle) {
    let Some(link) = link() else { return };
    let stream = match TcpStream::connect(("127.0.0.1", link.port)) {
        Ok(stream) => stream,
        Err(e) => {
            tracing::error!(error = %e, "could not reach YOBUKO; rbxport has no window of its own to show");
            app.exit(1);
            return;
        }
    };
    let _ = stream.set_nodelay(true);
    let (out, outgoing) = mpsc::channel::<Value>();
    if EMBED
        .set(Embed {
            out,
            bridges: Mutex::new(HashMap::new()),
        })
        .is_err()
    {
        return;
    }
    let mut writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(e) => {
            tracing::error!(error = %e, "could not use the connection to YOBUKO");
            app.exit(1);
            return;
        }
    };
    send(json!({ "t": "hello", "token": link.token, "version": env!("CARGO_PKG_VERSION") }));
    let ended = app.clone();
    let _ = std::thread::Builder::new()
        .name("yobuko-out".into())
        .spawn(move || {
            for message in outgoing {
                if let Err(e) = write_frame(&mut writer, &message) {
                    tracing::warn!(error = %e, "the connection to YOBUKO broke");
                    ended.exit(0);
                    return;
                }
            }
        });
    let reader_app = app.clone();
    let mut reader = stream;
    let _ = std::thread::Builder::new()
        .name("yobuko-in".into())
        .spawn(move || loop {
            match read_frame(&mut reader) {
                Ok(Some(message)) => handle(&reader_app, message),
                Ok(None) => {
                    tracing::info!("YOBUKO closed the connection");
                    reader_app.exit(0);
                    return;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "the connection to YOBUKO broke");
                    reader_app.exit(0);
                    return;
                }
            }
        });
    let menu_app = app.clone();
    let _ = std::thread::Builder::new()
        .name("yobuko-menu".into())
        .spawn(move || {
            let mut last = Value::Null;
            loop {
                let tree = menu_tree(&menu_app);
                if tree != last && !tree.is_null() {
                    send(json!({ "t": "menu", "tree": tree }));
                    last = tree;
                }
                std::thread::sleep(MENU_INTERVAL);
            }
        });
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn handle(app: &AppHandle, message: Value) {
    match message.get("t").and_then(Value::as_str).unwrap_or_default() {
        "invoke" => {
            let id = message
                .get("id")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let window = text(&message, "w");
            let command = text(&message, "cmd");
            let args = message.get("args").cloned().unwrap_or(Value::Null);
            let script = format!(
                "window.__yobukoBridge && window.__yobukoBridge.invoke({id}, {}, {});",
                Value::String(command),
                if args.is_null() { json!({}) } else { args }
            );
            if !run_in_bridge(app, &window, script) {
                send(json!({
                    "t": "reply", "id": id, "ok": false,
                    "value": { "kind": "Internal", "message": "This rbxport window is closed." }
                }));
            }
        }
        "asset" => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || serve_asset(&app, &message));
        }
        "menu" => {
            let id = text(&message, "id");
            crate::menu::on_event(app, &id);
        }
        "closed" => {
            let label = text(&message, "w");
            if label != crate::MAIN_WINDOW {
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.close();
                }
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Runs `script` in the bridge page of the window `label`, now or once it
/// has loaded. `false` when there is no such window.
fn run_in_bridge(app: &AppHandle, label: &str, script: String) -> bool {
    let Some(window) = app.get_webview_window(label) else {
        return false;
    };
    let Some(embed) = EMBED.get() else {
        return false;
    };
    let mut bridges = embed.bridges.lock();
    let bridge = bridges.entry(label.to_owned()).or_default();
    if bridge.ready {
        drop(bridges);
        let _ = window.eval(script);
    } else {
        bridge.queued.push(script);
    }
    true
}

/// Answers a request of YOBUKO's webview for a file: the interface's own
/// (`app`) or a picture or sound of the library (`rbl`).
fn serve_asset(app: &AppHandle, message: &Value) {
    let id = message
        .get("id")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let reply = |status: u16, headers: Vec<(String, String)>, body: &[u8]| {
        send(json!({
            "t": "asset", "id": id, "status": status, "headers": headers,
            "body": base64::engine::general_purpose::STANDARD.encode(body),
        }));
    };
    match text(message, "scheme").as_str() {
        "app" => {
            let path = text(message, "path");
            let path = if path.is_empty() || path == "/" {
                "/index.html".to_owned()
            } else {
                path
            };
            match app.asset_resolver().get(path) {
                Some(asset) => reply(
                    200,
                    vec![("Content-Type".into(), asset.mime_type)],
                    &asset.bytes,
                ),
                None => reply(404, Vec::new(), b""),
            }
        }
        "rbl" => {
            let mut request = tauri::http::Request::builder()
                .method(text(message, "method").as_str())
                .uri(text(message, "uri"));
            if let Some(headers) = message.get("headers").and_then(Value::as_array) {
                for pair in headers {
                    if let (Some(name), Some(value)) = (
                        pair.get(0).and_then(Value::as_str),
                        pair.get(1).and_then(Value::as_str),
                    ) {
                        request = request.header(name, value);
                    }
                }
            }
            let Ok(request) = request.body(Vec::new()) else {
                reply(400, Vec::new(), b"");
                return;
            };
            let state = Arc::clone(app.state::<Arc<crate::state::AppState>>().inner());
            crate::protocol::serve(&state, &request, |response| {
                let headers = response
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| {
                        Some((name.to_string(), value.to_str().ok()?.to_owned()))
                    })
                    .collect();
                reply(response.status().as_u16(), headers, response.body());
            });
        }
        _ => reply(404, Vec::new(), b""),
    }
}

/// The bridge page of `window` has loaded: what waited for it runs, and
/// YOBUKO opens the page of a window other than the main one.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects the calling window by value"
)]
pub fn yobuko_bridge_ready(window: tauri::WebviewWindow, hash: String) {
    let label = window.label().to_owned();
    let queued = EMBED.get().map(|embed| {
        let mut bridges = embed.bridges.lock();
        let bridge = bridges.entry(label.clone()).or_default();
        bridge.ready = true;
        std::mem::take(&mut bridge.queued)
    });
    for script in queued.unwrap_or_default() {
        let _ = window.eval(script);
    }
    let title = window.title().unwrap_or_default();
    send(json!({ "t": "page", "w": label, "title": title, "hash": hash }));
}

/// A command's answer, from a bridge page.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Tauri command's arguments are owned"
)]
pub fn yobuko_bridge_reply(id: u64, ok: bool, value: Value, b64: bool) {
    send(json!({ "t": "reply", "id": id, "ok": ok, "value": value, "b64": b64 }));
}

/// An event or a callback for the page in YOBUKO, from a bridge page.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri injects the calling window by value"
)]
pub fn yobuko_bridge_callback(window: tauri::WebviewWindow, id: u64, payload: Value) {
    send(json!({ "t": "callback", "w": window.label(), "id": id, "payload": payload }));
}

/// The audio files of tracks, for YOBUKO to drag out of its window.
#[tauri::command]
pub async fn yobuko_track_paths(
    state: tauri::State<'_, Arc<crate::state::AppState>>,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let library = state.library().map_err(|error| error.message)?;
    Ok(ids
        .iter()
        .filter_map(|id| library.audio_path_of(id).map(str::to_owned))
        .collect())
}

/// Brings `window` forward: in YOBUKO, its page.
pub fn focus<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    if embedded() {
        send(json!({ "t": "focus", "w": window.label() }));
    } else {
        let _ = window.set_focus();
    }
}

/// Shows `window`: in YOBUKO, its page.
pub fn show<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    if embedded() {
        send(json!({ "t": "focus", "w": window.label() }));
    } else {
        let _ = window.show();
    }
}

/// Runs `script` in the interface of `window`: in YOBUKO, in its page.
pub fn eval<R: Runtime>(window: &tauri::WebviewWindow<R>, script: String) {
    if embedded() {
        send(json!({ "t": "eval", "w": window.label(), "js": script }));
    } else {
        let _ = window.eval(script);
    }
}

/// A window has gone: YOBUKO closes its page.
pub fn window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if !embedded() || !matches!(event, tauri::WindowEvent::Destroyed) {
        return;
    }
    if let Some(embed) = EMBED.get() {
        embed.bridges.lock().remove(window.label());
    }
    send(json!({ "t": "gone", "w": window.label() }));
}

/// The menu bar as YOBUKO draws it. Items are `{"k":"item","id","text","on","key"}`,
/// submenus `{"k":"sub","id","text","on","items"}`, separators `{"k":"sep"}`
/// and the system's own items `{"k":"role","role","text"}` (`about`, `cut`,
/// `copy`, `paste`, `select-all`; the window's own ones, quit, hide and
/// close, are left out since the window is YOBUKO's).
pub fn menu_tree<R: Runtime>(app: &AppHandle<R>) -> Value {
    let Some(menu) = app.menu() else {
        return Value::Null;
    };
    let Ok(items) = menu.items() else {
        return Value::Null;
    };
    Value::Array(
        items
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| entry(kind, index == 0, false))
            .collect(),
    )
}

fn accelerator(id: &str) -> Value {
    ACCELERATORS
        .iter()
        .find(|(item, _)| *item == id)
        .map_or(Value::Null, |(_, key)| Value::String((*key).to_owned()))
}

fn entry<R: Runtime>(kind: &MenuItemKind<R>, application: bool, edit: bool) -> Option<Value> {
    match kind {
        MenuItemKind::MenuItem(item) => Some(json!({
            "k": "item", "id": item.id().0, "text": item.text().unwrap_or_default(),
            "on": item.is_enabled().unwrap_or(true), "key": accelerator(&item.id().0),
        })),
        MenuItemKind::Check(item) => Some(json!({
            "k": "item", "id": item.id().0, "text": item.text().unwrap_or_default(),
            "on": item.is_enabled().unwrap_or(true), "key": accelerator(&item.id().0),
            "checked": item.is_checked().unwrap_or(false),
        })),
        MenuItemKind::Icon(item) => Some(json!({
            "k": "item", "id": item.id().0, "text": item.text().unwrap_or_default(),
            "on": item.is_enabled().unwrap_or(true), "key": accelerator(&item.id().0),
        })),
        MenuItemKind::Submenu(sub) => {
            let edit = sub.id().0 == "edit";
            let items = sub.items().ok()?;
            let mut rows = Vec::new();
            let mut roles = 0usize;
            let count = items
                .iter()
                .filter(|kind| is_named_predefined(kind))
                .count();
            for kind in &items {
                if let MenuItemKind::Predefined(item) = kind {
                    let text = item.text().unwrap_or_default();
                    if text.is_empty() {
                        rows.push(json!({ "k": "sep" }));
                        continue;
                    }
                    let role = predefined_role(application, edit, roles, count);
                    roles += 1;
                    if let Some(role) = role {
                        rows.push(json!({ "k": "role", "role": role, "text": text }));
                    }
                    continue;
                }
                if let Some(row) = entry(kind, false, edit) {
                    rows.push(row);
                }
            }
            // Separators the left-out items leave doubled or at an end go too.
            let mut tidy: Vec<Value> = Vec::new();
            for row in rows {
                let separator = row.get("k").and_then(Value::as_str) == Some("sep");
                let after_separator = tidy
                    .last()
                    .is_none_or(|last| last.get("k").and_then(Value::as_str) == Some("sep"));
                if separator && after_separator {
                    continue;
                }
                tidy.push(row);
            }
            if tidy
                .last()
                .is_some_and(|last| last.get("k").and_then(Value::as_str) == Some("sep"))
            {
                tidy.pop();
            }
            Some(json!({
                "k": "sub", "id": sub.id().0, "text": sub.text().unwrap_or_default(),
                "on": sub.is_enabled().unwrap_or(true), "items": tidy,
            }))
        }
        MenuItemKind::Predefined(_) => {
            let _ = edit;
            None
        }
    }
}

fn is_named_predefined<R: Runtime>(kind: &MenuItemKind<R>) -> bool {
    matches!(kind, MenuItemKind::Predefined(item) if !item.text().unwrap_or_default().is_empty())
}

/// Which system item the `index`th named one of a submenu is, from the
/// order `menu.rs` builds them in; `None` for the window's own items.
pub fn predefined_role(
    application: bool,
    edit: bool,
    index: usize,
    count: usize,
) -> Option<&'static str> {
    if edit {
        return ["cut", "copy", "paste", "select-all"].get(index).copied();
    }
    if application && index == 0 {
        return Some("about");
    }
    let _ = count;
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_names_a_port_and_a_token() {
        assert_eq!(parse("51234:abc"), Some((51234, "abc".to_owned())));
        assert_eq!(parse("0:abc"), None);
        assert_eq!(parse("51234:"), None);
        assert_eq!(parse("nope"), None);
        assert_eq!(parse("70000:abc"), None);
    }

    #[test]
    fn a_frame_reads_back_as_it_was_written() {
        let mut bytes = Vec::new();
        let message = json!({ "t": "reply", "id": 7, "value": [1, 2, 3] });
        write_frame(&mut bytes, &message).unwrap();
        write_frame(&mut bytes, &json!({ "t": "quit" })).unwrap();
        let mut reader = std::io::Cursor::new(bytes);
        assert_eq!(read_frame(&mut reader).unwrap(), Some(message));
        assert_eq!(
            read_frame(&mut reader).unwrap(),
            Some(json!({ "t": "quit" }))
        );
        assert_eq!(read_frame(&mut reader).unwrap(), None);
    }

    #[test]
    fn a_frame_too_large_is_refused_before_it_is_read() {
        let mut bytes = u32::MAX.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        assert!(read_frame(&mut std::io::Cursor::new(bytes)).is_err());
    }

    #[test]
    fn every_key_given_to_yobuko_is_the_one_the_menu_has() {
        // Without white space, so a call wrapped over lines reads the same.
        let source: String = include_str!("menu.rs").split_whitespace().collect();
        for (id, key) in ACCELERATORS {
            let at = source.find(&format!("with_id(\"{id}\"")).unwrap();
            let rest = &source[at..];
            let end = rest.find(".build(app)").unwrap();
            assert!(
                rest[..end].contains(&format!(".accelerator(\"{key}\")")),
                "{id} is not on {key}"
            );
        }
        assert_eq!(
            source.matches(".accelerator(").count(),
            ACCELERATORS.len(),
            "a key in menu.rs is not given to YOBUKO"
        );
    }

    #[test]
    fn the_system_items_are_named_in_the_order_the_menu_builds_them() {
        assert_eq!(predefined_role(false, true, 0, 4), Some("cut"));
        assert_eq!(predefined_role(false, true, 3, 4), Some("select-all"));
        assert_eq!(predefined_role(true, false, 0, 3), Some("about"));
        assert_eq!(predefined_role(true, false, 2, 3), None);
        assert_eq!(predefined_role(false, false, 0, 1), None);
    }
}

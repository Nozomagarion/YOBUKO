// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO: rbxport inside this window. The USB space (`suhina_ui::app::usb_space`) leaves a
//! rectangle for rbxport; this puts rbxport's own interface there, in a webview that is a child
//! of the window, and runs the rbxport program hidden behind it (`link`), so every one of its
//! buttons, menus and windows is there and none of them opens a window of its own.
//!
//! - The interface's files and the library's pictures and sounds come from the rbxport program
//!   (`rbxport://` and `rbl://` here).
//! - Its commands go to the program and their answers come back (`shim.js` in the page,
//!   `yobuko_embed.rs` and `yobuko-bridge.js` in rbxport).
//! - A window rbxport opens (Preferences, the Sync Manager, Report bug, the Log) is a page of
//!   its own here, with a tab in the USB space.
//! - What belongs to a window is answered here: file dialogs (the system's, over this window),
//!   files dropped on the page or dragged out of it, copy and paste on macOS, full screen.

#[path = "rbxport_panel/link.rs"]
mod link;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use serde_json::{Value, json};
use suhina_ui::app::usb_space::{UsbAsk, UsbHost, UsbMenu, UsbPage, UsbStatus, UsbView, menus_from_tree};

/// What the page finds in place of Tauri.
const SHIM: &str = include_str!("rbxport_panel/shim.js");

/// Something a page did, heard by its webview's handlers and taken at the next frame.
enum FromPage {
    Message { label: String, body: String },
    Drop { label: String, kind: &'static str, paths: Vec<PathBuf>, at: (i32, i32) },
}

/// One of rbxport's windows, as a page in this one.
struct Page {
    label: String,
    title: String,
    webview: wry::WebView,
    bounds: Option<wry::Rect>,
    shown: bool,
    zoom: f32,
}

/// A dialog a page asked for, shown at the next frame over the window.
struct Dialog {
    label: String,
    id: u64,
    command: String,
    args: Value,
}

/// rbxport in the USB space.
pub struct RbxportPanel {
    link: Option<link::Link>,
    /// Why it stopped, until it is started again.
    failed: Option<String>,
    missing: bool,
    pages: Vec<Page>,
    active: String,
    menu_tree: Value,
    menus: Vec<UsbMenu>,
    heard: Arc<Mutex<Vec<FromPage>>>,
    /// Commands sent to rbxport, by the id they went with: the page and its own id.
    asked: HashMap<u64, (String, u64)>,
    next: u64,
    dialogs: Vec<Dialog>,
    contexts: Vec<wry::WebContext>,
    #[cfg(target_os = "linux")]
    gtk: bool,
}

impl Default for RbxportPanel {
    fn default() -> Self {
        Self {
            link: None,
            failed: None,
            missing: false,
            pages: Vec::new(),
            active: "main".into(),
            menu_tree: Value::Null,
            menus: Vec::new(),
            heard: Arc::new(Mutex::new(Vec::new())),
            asked: HashMap::new(),
            next: 1,
            dialogs: Vec::new(),
            contexts: Vec::new(),
            #[cfg(target_os = "linux")]
            gtk: false,
        }
    }
}

fn os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// The address of a file of `scheme` served to the webview, as each system's webview asks for it.
fn address(scheme: &str, path: &str) -> String {
    if cfg!(windows) { format!("http://{scheme}.localhost/{path}") } else { format!("{scheme}://localhost/{path}") }
}

/// Where the webviews keep their own data (WebView2 cannot write beside the program).
fn webview_data(label: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
    }?;
    let dir = base.join("YOBUKO").join("rbxport-webview");
    // WebKitGTK keeps one set of protocols per context: each page has its own on Linux.
    Some(if cfg!(target_os = "linux") { dir.join(label) } else { dir })
}

impl RbxportPanel {
    /// One frame: starts rbxport the first time the USB space shows, places its pages over
    /// `view`, runs what was picked on the page and passes on what came from rbxport.
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        shown: bool,
        view: Option<UsbView>,
        asks: Vec<UsbAsk>,
    ) {
        if shown && self.link.is_none() && self.failed.is_none() && !self.missing {
            self.start(ctx);
        }
        for ask in asks {
            self.ask(ask, ctx);
        }
        self.take_messages(ctx, frame);
        self.take_heard(ctx, frame);
        self.show_dialogs(frame);
        self.place(ctx, view.as_ref());
        #[cfg(target_os = "linux")]
        if self.gtk {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            if shown {
                // WebKitGTK's input and drawing run on GTK's loop, which turns with the frames.
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
    }

    /// What the USB space shows of rbxport.
    pub fn host(&self) -> UsbHost {
        let status = if self.missing {
            UsbStatus::Missing
        } else if let Some(why) = &self.failed {
            UsbStatus::Failed(why.clone())
        } else {
            match self.link.as_ref().map(link::Link::state) {
                None => UsbStatus::Off,
                Some(link::State::Waiting) => UsbStatus::Starting,
                Some(link::State::Connected) if self.pages.iter().any(|p| p.label == "main") => UsbStatus::Ready,
                Some(link::State::Connected) => UsbStatus::Starting,
                Some(link::State::Closed(why)) => UsbStatus::Failed(why),
            }
        };
        UsbHost {
            status,
            pages: self.pages.iter().map(|p| UsbPage { label: p.label.clone(), title: p.title.clone() }).collect(),
            active: self.active.clone(),
            menus: self.menus.clone(),
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(program) = suhina_ui::app::usb_space::rbxport_program() else {
            self.missing = true;
            return;
        };
        #[cfg(target_os = "macos")]
        if let Some(app) = program.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
            // The copy inside YOBUKO.app arrives quarantined from the disk image; YOBUKO itself was
            // allowed, so its helper is cleared too, or macOS refuses to run it.
            let _ = std::process::Command::new("xattr")
                .args(["-dr", "com.apple.quarantine"])
                .arg(app)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(target_os = "linux")]
        if !self.gtk {
            match gtk::init() {
                Ok(()) => self.gtk = true,
                Err(e) => {
                    self.failed = Some(format!("the system's web view could not start ({e})."));
                    return;
                }
            }
        }
        let wake = ctx.clone();
        match link::Link::start(&program, move || wake.request_repaint()) {
            Ok(link) => self.link = Some(link),
            Err(e) => self.failed = Some(format!("could not start rbxport: {e}")),
        }
    }

    /// Ends rbxport and its pages; the next frame of the USB space starts it again.
    fn restart(&mut self) {
        self.pages.clear();
        self.contexts.clear();
        self.asked.clear();
        self.link = None;
        self.failed = None;
        self.active = "main".into();
    }

    fn send(&self, message: Value) {
        if let Some(link) = &self.link {
            link.send(message);
        }
    }

    fn page(&self, label: &str) -> Option<&Page> {
        self.pages.iter().find(|p| p.label == label)
    }

    fn eval(&self, label: &str, script: &str) {
        if let Some(page) = self.page(label) {
            let _ = page.webview.evaluate_script(script);
        }
    }

    fn reply(&self, label: &str, id: u64, ok: bool, value: &Value, b64: bool) {
        self.eval(label, &format!("window.__yobukoShim && window.__yobukoShim.reply({id}, {ok}, {value}, {b64});"));
    }

    fn ask(&mut self, ask: UsbAsk, ctx: &egui::Context) {
        match ask {
            UsbAsk::Menu(id) => self.menu(&id, ctx),
            UsbAsk::Role(role) => {
                let action = match role.as_str() {
                    "select-all" => "selectAll",
                    "cut" => "cut",
                    "copy" => "copy",
                    "paste" => "paste",
                    _ => return,
                };
                if let Some(page) = self.page(&self.active) {
                    let _ = page.webview.focus();
                }
                self.eval(&self.active, &format!("window.__yobukoShim && window.__yobukoShim.edit({action:?});"));
            }
            UsbAsk::Show(label) => {
                if self.page(&label).is_some() {
                    self.active = label;
                }
            }
            UsbAsk::Close(label) => self.close(&label),
            UsbAsk::Restart => self.restart(),
        }
    }

    /// A menu item of rbxport's: full screen is this window's, the rest is rbxport's.
    fn menu(&self, id: &str, ctx: &egui::Context) {
        if id == "fullscreen" {
            let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
        } else {
            self.send(json!({ "t": "menu", "id": id }));
        }
    }

    fn close(&mut self, label: &str) {
        if label == "main" {
            return;
        }
        self.pages.retain(|p| p.label != label);
        if self.active == label {
            self.active = "main".into();
        }
        self.send(json!({ "t": "closed", "w": label }));
    }

    /// What rbxport sent.
    fn take_messages(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(link) = &self.link else { return };
        let state = link.state();
        let messages = link.take();
        if state == link::State::Connected && self.page("main").is_none() {
            self.open(frame, ctx, "main", "rbxport", "");
        }
        for message in messages {
            let text = |key: &str| message.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
            match text("t").as_str() {
                "reply" => {
                    let id = message.get("id").and_then(Value::as_u64).unwrap_or_default();
                    if let Some((label, page_id)) = self.asked.remove(&id) {
                        let ok = message.get("ok").and_then(Value::as_bool).unwrap_or(false);
                        let b64 = message.get("b64").and_then(Value::as_bool).unwrap_or(false);
                        self.reply(&label, page_id, ok, message.get("value").unwrap_or(&Value::Null), b64);
                    }
                }
                "callback" => {
                    let id = message.get("id").and_then(Value::as_u64).unwrap_or_default();
                    let payload = message.get("payload").unwrap_or(&Value::Null);
                    self.eval(
                        &text("w"),
                        &format!("window.__yobukoShim && window.__yobukoShim.callback({id}, {payload});"),
                    );
                }
                "page" => {
                    let label = text("w");
                    if label != "main" {
                        if self.page(&label).is_none() {
                            self.open(frame, ctx, &label, &text("title"), &text("hash"));
                        }
                        self.active = label;
                    }
                }
                "focus" => {
                    let label = text("w");
                    if self.page(&label).is_some() {
                        self.active = label;
                    }
                }
                "gone" => {
                    let label = text("w");
                    if label != "main" {
                        self.pages.retain(|p| p.label != label);
                        if self.active == label {
                            self.active = "main".into();
                        }
                    }
                }
                "eval" => self.eval(&text("w"), &text("js")),
                "menu" => {
                    let tree = message.get("tree").cloned().unwrap_or(Value::Null);
                    if tree != self.menu_tree {
                        self.menus = menus_from_tree(&tree, cfg!(target_os = "macos"));
                        self.menu_tree = tree;
                        let keys = self.keys();
                        for page in &self.pages {
                            let _ = page
                                .webview
                                .evaluate_script(&format!("window.__yobukoShim && window.__yobukoShim.keys({keys});"));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The menu's keys, for the pages: `[{id, key}]`.
    fn keys(&self) -> Value {
        fn walk(items: &Value, out: &mut Vec<Value>) {
            for item in items.as_array().into_iter().flatten() {
                if let (Some(id), Some(key)) =
                    (item.get("id").and_then(Value::as_str), item.get("key").and_then(Value::as_str))
                {
                    out.push(json!({ "id": id, "key": key }));
                }
                if let Some(inner) = item.get("items") {
                    walk(inner, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.menu_tree, &mut out);
        Value::Array(out)
    }

    /// Makes the page for rbxport's window `label`.
    fn open(&mut self, frame: &eframe::Frame, ctx: &egui::Context, label: &str, title: &str, hash: &str) {
        let Some(link) = &self.link else { return };
        let config = json!({ "label": label, "os": os(), "keys": self.keys() });
        let mut script = format!("window.__YOBUKO_RBX__ = {config};\n{SHIM}\n");
        if !hash.is_empty() {
            script.push_str(&format!(
                "if (window === window.top && !location.hash) location.hash = {};\n",
                Value::String(hash.to_owned())
            ));
        }
        let heard = self.heard.clone();
        let wake = ctx.clone();
        let from = label.to_owned();
        let on_message = move |request: http::Request<String>| {
            if let Ok(mut heard) = heard.lock() {
                heard.push(FromPage::Message { label: from.clone(), body: request.into_body() });
            }
            wake.request_repaint();
        };
        let heard = self.heard.clone();
        let wake = ctx.clone();
        let from = label.to_owned();
        let on_drop = move |event: wry::DragDropEvent| {
            let (kind, paths, at) = match event {
                wry::DragDropEvent::Enter { paths, position } => ("enter", paths, position),
                wry::DragDropEvent::Over { position } => ("over", Vec::new(), position),
                wry::DragDropEvent::Drop { paths, position } => ("drop", paths, position),
                wry::DragDropEvent::Leave => ("leave", Vec::new(), (0, 0)),
                _ => return false,
            };
            if let Ok(mut heard) = heard.lock() {
                heard.push(FromPage::Drop { label: from.clone(), kind, paths, at });
            }
            wake.request_repaint();
            // The page's own drop still happens.
            false
        };
        let app_files = link.requests();
        let library = link.requests();
        let serve_app =
            move |_id: wry::WebViewId, request: http::Request<Vec<u8>>, responder: wry::RequestAsyncResponder| {
                let path = request.uri().path().to_owned();
                if std::env::var_os("YOBUKO_RBXPORT_DEBUG").is_some() {
                    eprintln!("rbxport file asked: {}", request.uri());
                }
                app_files.ask(json!({ "scheme": "app", "path": path }), answer(responder));
            };
        let serve_library =
            move |_id: wry::WebViewId, request: http::Request<Vec<u8>>, responder: wry::RequestAsyncResponder| {
                let headers: Vec<(String, String)> = request
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_owned())))
                    .collect();
                library.ask(
                    json!({
                        "scheme": "rbl", "uri": request.uri().to_string(),
                        "method": request.method().as_str(), "headers": headers,
                    }),
                    answer(responder),
                );
            };
        let mut context = wry::WebContext::new(webview_data(label));
        let shared = !cfg!(target_os = "linux") && !self.contexts.is_empty();
        let built = {
            let context = if shared { self.contexts.first_mut() } else { Some(&mut context) };
            let Some(context) = context else { return };
            let builder = wry::WebViewBuilder::new_with_web_context(context)
                .with_url(address("rbxport", "index.html"))
                .with_initialization_script(script)
                .with_ipc_handler(on_message)
                .with_drag_drop_handler(on_drop)
                .with_asynchronous_custom_protocol("rbxport".into(), serve_app)
                .with_asynchronous_custom_protocol("rbl".into(), serve_library)
                .with_visible(false)
                .with_accept_first_mouse(true)
                .with_hotkeys_zoom(false)
                .with_devtools(cfg!(debug_assertions))
                // X11 refuses a window of no size: one pixel until the page is placed.
                .with_bounds(wry::Rect {
                    position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                    size: wry::dpi::PhysicalSize::new(1u32, 1u32).into(),
                });
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.build_as_child(frame)))
        };
        if !shared {
            self.contexts.push(context);
        }
        match built {
            Ok(Ok(webview)) => self.pages.push(Page {
                label: label.to_owned(),
                title: title.to_owned(),
                webview,
                bounds: None,
                shown: false,
                zoom: 1.0,
            }),
            Ok(Err(e)) => self.failed = Some(format!("the system's web view could not open: {e}")),
            Err(_) => self.failed = Some("the system's web view could not open in this window.".into()),
        }
    }

    /// What the pages did.
    fn take_heard(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let heard: Vec<FromPage> = self.heard.lock().map(|mut h| std::mem::take(&mut *h)).unwrap_or_default();
        for event in heard {
            match event {
                FromPage::Drop { label, kind, paths, at } => {
                    let paths: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
                    self.eval(
                        &label,
                        &format!(
                            "window.__yobukoShim && window.__yobukoShim.drop({kind:?}, {}, {}, {});",
                            json!(paths),
                            at.0,
                            at.1
                        ),
                    );
                }
                FromPage::Message { label, body } => {
                    let Ok(message) = serde_json::from_str::<Value>(&body) else { continue };
                    self.heard_from_page(&label, message, ctx, frame);
                }
            }
        }
    }

    fn heard_from_page(&mut self, label: &str, message: Value, ctx: &egui::Context, frame: &eframe::Frame) {
        let id = message.get("id").and_then(Value::as_u64).unwrap_or_default();
        let text = |key: &str| message.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
        match text("t").as_str() {
            "invoke" => {
                let sent = self.next;
                self.next += 1;
                self.asked.insert(sent, (label.to_owned(), id));
                let args = message.get("args").cloned().unwrap_or(json!({}));
                self.send(json!({ "t": "invoke", "id": sent, "w": label, "cmd": text("cmd"), "args": args }));
            }
            "dialog" => self.dialogs.push(Dialog {
                label: label.to_owned(),
                id,
                command: text("cmd"),
                args: message.get("args").cloned().unwrap_or(Value::Null),
            }),
            "drag" => {
                let paths: Vec<PathBuf> = message
                    .get("paths")
                    .and_then(Value::as_array)
                    .map(|paths| paths.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
                    .unwrap_or_default();
                match crate::drag_out(frame, paths) {
                    Ok(_) => self.reply(label, id, true, &Value::Null, false),
                    Err(e) => self.reply(label, id, false, &json!(e), false),
                }
            }
            "menu" => self.menu(&text("menu"), ctx),
            "close" => self.close(label),
            "maximize" => {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            "log" => {
                if suhina_plugins::trace::on() || std::env::var_os("YOBUKO_RBXPORT_DEBUG").is_some() {
                    eprintln!("rbxport page {label}: {}", text("text"));
                }
            }
            "paste" => {
                let pasted = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
                self.eval(label, &format!("window.__yobukoShim && window.__yobukoShim.paste({});", json!(pasted)));
            }
            _ => {}
        }
    }

    /// The file and message dialogs the pages asked for, over this window.
    fn show_dialogs(&mut self, frame: &eframe::Frame) {
        for dialog in std::mem::take(&mut self.dialogs) {
            let answer = dialog_answer(&dialog.command, &dialog.args, frame);
            self.reply(&dialog.label, dialog.id, true, &answer, false);
        }
    }

    /// Puts the page in front over `view`, the others out of sight.
    fn place(&mut self, ctx: &egui::Context, view: Option<&UsbView>) {
        let scale = ctx.pixels_per_point();
        let zoom = ctx.zoom_factor();
        for page in &mut self.pages {
            let wanted = view.filter(|v| !v.covered && page.label == self.active);
            match wanted {
                Some(view) => {
                    let rect = view.rect;
                    let bounds = wry::Rect {
                        position: wry::dpi::PhysicalPosition::new(
                            (rect.left() * scale).round() as i32,
                            (rect.top() * scale).round() as i32,
                        )
                        .into(),
                        size: wry::dpi::PhysicalSize::new(
                            (rect.width() * scale).round().max(1.0) as u32,
                            (rect.height() * scale).round().max(1.0) as u32,
                        )
                        .into(),
                    };
                    // Shown first and placed after: on Linux, showing the webview again
                    // lays it out anew, so the size given before would be lost.
                    if !page.shown {
                        let _ = page.webview.set_visible(true);
                        page.shown = true;
                        page.bounds = None;
                    }
                    if page.bounds != Some(bounds) {
                        let _ = page.webview.set_bounds(bounds);
                        page.bounds = Some(bounds);
                    }
                    if (page.zoom - zoom).abs() > f32::EPSILON {
                        let _ = page.webview.zoom(f64::from(zoom));
                        page.zoom = zoom;
                    }
                }
                None if page.shown => {
                    let _ = page.webview.set_visible(false);
                    let _ = page.webview.focus_parent();
                    page.shown = false;
                }
                None => {}
            }
        }
    }
}

/// A responder for one of the webview's file requests.
fn answer(responder: wry::RequestAsyncResponder) -> link::Responder {
    Box::new(move |status: u16, headers: Vec<(String, String)>, body: Vec<u8>| {
        let mut response = http::Response::builder().status(status).header("Access-Control-Allow-Origin", "*");
        for (name, value) in headers {
            response = response.header(name, value);
        }
        let response = response.body(body).unwrap_or_else(|_| http::Response::new(Vec::new()));
        responder.respond(response);
    })
}

/// What a dialog of rbxport's (Tauri's dialog plug-in) answers, shown with the system's
/// dialogs over this window: a path, paths or `null` for files, the button's name for messages.
fn dialog_answer(command: &str, args: &Value, frame: &eframe::Frame) -> Value {
    let options = args.get("options").unwrap_or(args);
    let field = |key: &str| options.get(key).and_then(Value::as_str).map(str::to_owned);
    let flag = |key: &str| options.get(key).and_then(Value::as_bool).unwrap_or(false);
    let path_list = |paths: Option<Vec<PathBuf>>| {
        paths.map_or(Value::Null, |p| Value::Array(p.iter().map(|p| json!(p.display().to_string())).collect()))
    };
    let one = |path: Option<PathBuf>| path.map_or(Value::Null, |p| json!(p.display().to_string()));
    match command {
        "open" | "save" => {
            let mut dialog = rfd::FileDialog::new().set_parent(frame);
            if let Some(title) = field("title") {
                dialog = dialog.set_title(title);
            }
            if let Some(start) = field("defaultPath").map(PathBuf::from) {
                if start.is_dir() {
                    dialog = dialog.set_directory(&start);
                } else {
                    if let Some(dir) = start.parent().filter(|d| d.is_dir()) {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(name) = start.file_name() {
                        dialog = dialog.set_file_name(name.to_string_lossy());
                    }
                }
            }
            for filter in options.get("filters").and_then(Value::as_array).into_iter().flatten() {
                let name = filter.get("name").and_then(Value::as_str).unwrap_or_default();
                let extensions: Vec<&str> = filter
                    .get("extensions")
                    .and_then(Value::as_array)
                    .map(|e| e.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if !extensions.is_empty() {
                    dialog = dialog.add_filter(name, &extensions);
                }
            }
            if command == "save" {
                one(dialog.save_file())
            } else {
                match (flag("directory"), flag("multiple")) {
                    (true, true) => path_list(dialog.pick_folders()),
                    (true, false) => one(dialog.pick_folder()),
                    (false, true) => path_list(dialog.pick_files()),
                    (false, false) => one(dialog.pick_file()),
                }
            }
        }
        _ => {
            let message = args.get("message").and_then(Value::as_str).unwrap_or_default();
            let title = args.get("title").and_then(Value::as_str).unwrap_or("rbxport");
            let level = match args.get("kind").and_then(Value::as_str) {
                Some("warning") => rfd::MessageLevel::Warning,
                Some("error") => rfd::MessageLevel::Error,
                _ => rfd::MessageLevel::Info,
            };
            let buttons = message_buttons(args.get("buttons"), command);
            let result = rfd::MessageDialog::new()
                .set_parent(frame)
                .set_title(title)
                .set_description(message)
                .set_level(level)
                .set_buttons(buttons)
                .show();
            let named = match result {
                rfd::MessageDialogResult::Ok => "Ok".to_owned(),
                rfd::MessageDialogResult::Cancel => "Cancel".to_owned(),
                rfd::MessageDialogResult::Yes => "Yes".to_owned(),
                rfd::MessageDialogResult::No => "No".to_owned(),
                rfd::MessageDialogResult::Custom(name) => name,
            };
            match command {
                "ask" => json!(named == "Yes"),
                "confirm" => json!(named == "Ok"),
                _ => json!(named),
            }
        }
    }
}

/// The buttons of a message, as Tauri's dialog plug-in names them.
fn message_buttons(buttons: Option<&Value>, command: &str) -> rfd::MessageButtons {
    let text = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or_default().to_owned();
    match buttons {
        Some(Value::String(kind)) => match kind.as_str() {
            "OkCancel" => rfd::MessageButtons::OkCancel,
            "YesNo" => rfd::MessageButtons::YesNo,
            "YesNoCancel" => rfd::MessageButtons::YesNoCancel,
            _ => rfd::MessageButtons::Ok,
        },
        Some(Value::Object(custom)) => {
            if let Some(ok) = custom.get("OkCustom") {
                rfd::MessageButtons::OkCustom(text(Some(ok)))
            } else if let Some(pair) = custom.get("OkCancelCustom") {
                rfd::MessageButtons::OkCancelCustom(text(pair.get(0)), text(pair.get(1)))
            } else if let Some(three) = custom.get("YesNoCancelCustom") {
                rfd::MessageButtons::YesNoCancelCustom(text(three.get(0)), text(three.get(1)), text(three.get(2)))
            } else {
                rfd::MessageButtons::Ok
            }
        }
        _ if command == "ask" => rfd::MessageButtons::YesNo,
        _ if command == "confirm" => rfd::MessageButtons::OkCancel,
        _ => rfd::MessageButtons::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_system_asks_for_the_interface_at_its_own_address() {
        let at = address("rbxport", "index.html");
        if cfg!(windows) {
            assert_eq!(at, "http://rbxport.localhost/index.html");
        } else {
            assert_eq!(at, "rbxport://localhost/index.html");
        }
    }

    #[test]
    fn the_dialog_buttons_follow_tauris_names() {
        assert!(matches!(message_buttons(Some(&json!("YesNo")), "message"), rfd::MessageButtons::YesNo));
        assert!(matches!(
            message_buttons(Some(&json!({ "OkCancelCustom": ["Replace", "Keep"] })), "message"),
            rfd::MessageButtons::OkCancelCustom(ok, cancel) if ok == "Replace" && cancel == "Keep"
        ));
        assert!(matches!(message_buttons(None, "ask"), rfd::MessageButtons::YesNo));
        assert!(matches!(message_buttons(None, "message"), rfd::MessageButtons::Ok));
    }

    #[test]
    fn webview_data_stays_in_yobukos_own_folder() {
        if let Some(dir) = webview_data("main") {
            assert!(dir.to_string_lossy().contains("YOBUKO"));
        }
    }

    #[test]
    fn the_shim_stands_in_for_everything_the_interface_takes_from_tauri() {
        for needed in [
            "__TAURI_INTERNALS__",
            "__TAURI_EVENT_PLUGIN_INTERNALS__",
            "transformCallback",
            "convertFileSrc",
            "metadata",
            "plugin:dialog|",
            "dropped_file_paths",
            "drag_tracks",
            "__yobukoShim",
        ] {
            assert!(SHIM.contains(needed), "{needed}");
        }
    }
}

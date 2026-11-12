// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//
// YOBUKO: what rbxport's interface finds in place of Tauri when it is drawn inside YOBUKO's
// window (`rbxport_panel.rs`). Its commands go to the rbxport program through YOBUKO
// (`window.ipc.postMessage`); answers, events and callbacks come back through
// `window.__yobukoShim`. What belongs to the window (its size, closing it, file dialogs,
// dropped and dragged files) is answered by YOBUKO itself. `__YOBUKO_RBX__` ({label, os}) is
// set just before this script.
(() => {
  "use strict";
  const page = window.__YOBUKO_RBX__ || { label: "main", os: "linux" };
  const windows = page.os === "windows";
  const mac = page.os === "macos";
  const post = (message) => window.ipc.postMessage(JSON.stringify(message));
  const IPC_KEY = "__TAURI_TO_IPC_KEY__";

  const callbacks = new Map();
  const transformCallback = (callback, once = false) => {
    const id = window.crypto.getRandomValues(new Uint32Array(1))[0];
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      return callback && callback(data);
    });
    return id;
  };
  const runCallback = (id, data) => {
    const callback = callbacks.get(id);
    if (callback) callback(data);
  };

  let next = 1;
  const pending = new Map();
  const ask = (message) => new Promise((resolve, reject) => {
    const id = next++;
    pending.set(id, { resolve, reject });
    post({ ...message, id });
  });
  const serialize = (value) => value === undefined ? {} : JSON.parse(JSON.stringify(value, (_key, item) =>
    item && typeof item === "object" && typeof item[IPC_KEY] === "function" ? item[IPC_KEY]() : item));
  const fromBase64 = (text) => {
    const raw = atob(text);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
    return bytes.buffer;
  };

  // Files dropped on the page: YOBUKO hears the drop with its paths.
  const local = new Map();
  let localId = -1;
  let lastDrop = null;
  let dropWaiters = [];
  const DRAG = { enter: "tauri://drag-enter", over: "tauri://drag-over", drop: "tauri://drag-drop", leave: "tauri://drag-leave" };
  const droppedPaths = (names) => {
    const pick = (paths) => {
      if (!Array.isArray(names) || names.length === 0) return paths;
      const base = (path) => path.split(/[\\/]/).pop();
      const found = names.map((name) => paths.find((path) => base(path) === name || base(path).normalize("NFC") === String(name).normalize("NFC")));
      return found.every(Boolean) ? found : paths;
    };
    if (lastDrop && Date.now() - lastDrop.at < 3000) return Promise.resolve(pick(lastDrop.paths));
    return new Promise((resolve, reject) => {
      const waiter = { resolve: (paths) => resolve(pick(paths)) };
      dropWaiters.push(waiter);
      setTimeout(() => {
        if (dropWaiters.includes(waiter)) {
          dropWaiters = dropWaiters.filter((w) => w !== waiter);
          reject(new Error("This platform did not provide the dropped files' locations."));
        }
      }, 4000);
    });
  };

  const windowCommand = (name, args) => {
    const ratio = window.devicePixelRatio || 1;
    switch (name) {
      case "scale_factor": return ratio;
      case "close": case "destroy": post({ t: "close" }); return null;
      case "toggle_maximize": case "maximize": post({ t: "maximize" }); return null;
      case "is_fullscreen": case "is_maximized": case "is_minimized": return false;
      case "is_visible": case "is_enabled": case "is_resizable": case "is_decorated": return true;
      case "is_focused": return document.hasFocus();
      case "title": return document.title;
      case "set_title": if (args && typeof args.value === "string") document.title = args.value; return null;
      case "inner_size": case "outer_size": return { width: Math.round(innerWidth * ratio), height: Math.round(innerHeight * ratio) };
      case "inner_position": case "outer_position": return { x: 0, y: 0 };
      case "theme": return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
      default: return null;
    }
  };

  async function invoke(cmd, args = {}, _options) {
    if (cmd === "show_window" || cmd === "startup_milestone") return null;
    if (cmd.startsWith("plugin:window|")) return windowCommand(cmd.slice(14), args);
    if (cmd.startsWith("plugin:webview|")) return windowCommand(cmd.slice(15), args);
    if (cmd === "plugin:event|listen" && Object.values(DRAG).includes(args.event)) {
      const id = localId--;
      local.set(id, { event: args.event, handler: args.handler });
      return id;
    }
    if (cmd === "plugin:event|unlisten" && local.has(args.eventId)) {
      local.delete(args.eventId);
      return null;
    }
    if (cmd.startsWith("plugin:dialog|")) return ask({ t: "dialog", cmd: cmd.slice(14), args: serialize(args) });
    if (cmd === "dropped_file_paths") return droppedPaths(args && args.names);
    if (cmd === "drag_tracks") {
      const paths = await ask({ t: "invoke", cmd: "yobuko_track_paths", args: { ids: (args && args.ids) || [] } });
      return ask({ t: "drag", paths });
    }
    return ask({ t: "invoke", cmd, args: serialize(args) });
  }

  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    unregisterCallback: (id) => callbacks.delete(id),
    runCallback,
    callbacks,
    convertFileSrc: (path, protocol = "asset") => {
      const encoded = encodeURIComponent(path);
      return windows ? `http://${protocol}.localhost/${encoded}` : `${protocol}://localhost/${encoded}`;
    },
    metadata: {
      currentWindow: { label: page.label },
      currentWebview: { windowLabel: page.label, label: page.label },
    },
    plugins: { path: { sep: windows ? "\\" : "/", delimiter: windows ? ";" : ":" } },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };

  // WebView2 would hand dropped files to a host that is not listening: the interface asks
  // `dropped_file_paths` instead, which YOBUKO answers.
  try {
    if (window.chrome && window.chrome.webview) {
      Object.defineProperty(window.chrome.webview, "postMessageWithAdditionalObjects", { value: undefined, configurable: true });
    }
  } catch (_) { /* not WebView2 */ }

  // The menu's keys, pressed in the page, run its items as the native menu's would.
  let keys = Array.isArray(page.keys) ? page.keys : [];
  const parse = (accelerator) => {
    const parts = accelerator.split("+");
    const key = parts.pop().toLowerCase();
    const mods = new Set(parts.map((part) => part.toLowerCase()));
    return { key, cmd: mods.has("cmdorctrl") || mods.has("cmd") || mods.has("ctrl"), shift: mods.has("shift"), alt: mods.has("alt") };
  };
  const editable = (element) => element && (element.isContentEditable || /^(input|textarea|select)$/i.test(element.tagName));
  window.addEventListener("keydown", (event) => {
    const command = mac ? event.metaKey : event.ctrlKey;
    if (!command) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key.toLowerCase();
    for (const { id, key: accelerator } of keys) {
      const want = parse(accelerator);
      if (want.key === key && want.cmd && want.shift === event.shiftKey && want.alt === event.altKey) {
        if (editable(document.activeElement) && (id === "undo" || id === "redo")) return;
        event.preventDefault();
        event.stopPropagation();
        post({ t: "menu", menu: id });
        return;
      }
    }
    // macOS gives a webview its copy and paste keys through the application's Edit menu,
    // which is YOBUKO's: text fields get them here.
    if (mac && !event.shiftKey && !event.altKey && editable(document.activeElement)) {
      const action = { c: "copy", x: "cut", a: "selectAll" }[key];
      if (action) {
        event.preventDefault();
        document.execCommand(action);
      } else if (key === "v") {
        event.preventDefault();
        post({ t: "paste" });
      } else if (key === "z") {
        event.preventDefault();
        document.execCommand(event.shiftKey ? "redo" : "undo");
      }
    }
  }, true);

  // Errors of the page reach YOBUKO's trace (`suhina --trace`), since the page has no console.
  const report = (text) => { try { post({ t: "log", text: String(text).slice(0, 2000) }); } catch (_) { /* gone */ } };
  window.addEventListener("error", (event) => report(`${event.message} at ${event.filename}:${event.lineno}`));
  window.addEventListener("unhandledrejection", (event) => report(`unhandled: ${event.reason && (event.reason.message || JSON.stringify(event.reason))}`));
  const consoleError = console.error.bind(console);
  console.error = (...args) => { report(args.map((a) => (a && a.message) || String(a)).join(" ")); consoleError(...args); };

  window.__yobukoShim = {
    reply(id, ok, value, b64) {
      const waiting = pending.get(id);
      if (!waiting) return;
      pending.delete(id);
      const result = b64 && typeof value === "string" ? fromBase64(value) : value;
      if (ok) waiting.resolve(result);
      else waiting.reject(result);
    },
    callback(id, payload) {
      runCallback(id, payload);
    },
    drop(kind, paths, x, y) {
      const event = DRAG[kind];
      if (!event) return;
      if (kind === "drop" && paths.length > 0) {
        lastDrop = { paths, at: Date.now() };
        const waiters = dropWaiters;
        dropWaiters = [];
        for (const waiter of waiters) waiter.resolve(paths);
      }
      for (const [id, entry] of local) {
        if (entry.event === event) runCallback(entry.handler, { event, id, payload: { paths, position: { x, y } } });
      }
    },
    keys(list) {
      keys = Array.isArray(list) ? list : [];
    },
    paste(text) {
      if (typeof text === "string" && text) document.execCommand("insertText", false, text);
    },
    edit(action) {
      if (action === "paste") post({ t: "paste" });
      else document.execCommand(action);
    },
  };
})();

// YOBUKO patch (not in upstream rbxport): the bridge between rbxport's
// commands and its interface drawn inside YOBUKO's window.
//
// YOBUKO's page asks for a command; src-tauri/src/yobuko_embed.rs evaluates
// `__yobukoBridge.invoke(id, cmd, args)` here, in the hidden window of the
// same name, so the command runs with Tauri's own invoke from the window it
// expects; the answer goes back through `yobuko_bridge_reply`. A listener or
// channel YOBUKO's page passes (a callback id of its own) is replaced by one
// of this page that hands every call on through `yobuko_bridge_callback`.
(() => {
  "use strict";
  const T = window.__TAURI_INTERNALS__;
  if (!T) return;
  const CHANNEL = /^__CHANNEL__:(\d+)$/;

  const base64 = (bytes) => {
    let text = "";
    for (let i = 0; i < bytes.length; i += 0x8000) {
      text += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    }
    return btoa(text);
  };

  const relay = (id) => T.transformCallback((payload) => {
    T.invoke("yobuko_bridge_callback", { id, payload: payload === undefined ? null : payload }).catch(() => {});
  });

  const rewrite = (cmd, args) => {
    if (!args || typeof args !== "object" || Array.isArray(args)) return args;
    const out = {};
    for (const [key, value] of Object.entries(args)) {
      const channel = typeof value === "string" ? CHANNEL.exec(value) : null;
      out[key] = channel ? `__CHANNEL__:${relay(Number(channel[1]))}` : value;
    }
    if (cmd === "plugin:event|listen" && typeof args.handler === "number") out.handler = relay(args.handler);
    return out;
  };

  const reply = async (id, ok, value) => {
    let b64 = false;
    if (value instanceof ArrayBuffer) {
      value = base64(new Uint8Array(value));
      b64 = true;
    } else if (ArrayBuffer.isView(value)) {
      value = base64(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
      b64 = true;
    } else if (value instanceof Error) {
      value = value.message;
    }
    if (value === undefined) value = null;
    try {
      await T.invoke("yobuko_bridge_reply", { id, ok, value, b64 });
    } catch (error) {
      await T.invoke("yobuko_bridge_reply", { id, ok: false, value: String(error), b64: false }).catch(() => {});
    }
  };

  window.__yobukoBridge = {
    invoke(id, cmd, args) {
      Promise.resolve()
        .then(() => T.invoke(cmd, rewrite(cmd, args)))
        .then((value) => reply(id, true, value), (error) => reply(id, false, error));
    },
  };

  T.invoke("yobuko_bridge_ready", { hash: location.hash || "" }).catch(() => {});
})();

/**
 * YOBUKO patch (not in upstream rbxport): the window takes the colours of the
 * theme YOBUKO's studio uses, so rbxport reads as part of the same app.
 *
 * The studio writes them to `<data dir>/YOBUKO/look.json` (YOBUKO's
 * `crates/dj/src/look.rs`); the `yobuko_look` command hands the file over. The
 * interface's grey tokens follow their lightness up the look's own ramp
 * (ground, panels, fields, hints, text), so a light theme turns rbxport light
 * too, and its blue highlights take the look's accent: the same rule YOBUKO's
 * Mixxx skin is recoloured with (`crates/dj/src/mixxx_skin.rs`, `map`). Data
 * colours (waveforms, cue, phrase and colour-label colours) keep rekordbox's
 * meaning and are left alone.
 */

export type Look = {
  dark: boolean;
  ground: string;
  surface: string;
  control: string;
  controlHover: string;
  controlActive: string;
  lineStrong: string;
  text: string;
  textDim: string;
  hint: string;
  inverse: string;
  accent: string;
  recording: string;
  meterNormal: string;
  meterHot: string;
};

type Rgb = { r: number; g: number; b: number };

/** The tokens that mark what is chosen or lit, rekordbox's blues: they take the accent. */
export const ACCENT_TOKENS = [
  "--c-accent", "--c-pad-active", "--c-tab-active", "--c-filter-accent", "--c-device-tab-on",
  "--c-info-tab-active", "--c-rail-lit", "--c-pref-select", "--c-search-focus", "--c-meter-fill",
  "--c-bars-label", "--c-pad-edit-on", "--c-sync-pending",
] as const;

/** Grey tokens that sit on a lit accent fill: they take the text for lit fills. */
const ON_ACCENT = ["--c-info-tab-active-text"] as const;

/** Alarm and level colours, which keep their meaning in the look's own red, green and yellow. */
const SEMANTIC: Record<string, keyof Look> = {
  "--c-error": "recording",
  "--c-meter-red": "recording",
  "--c-meter-green": "meterNormal",
  "--c-meter-yellow": "meterHot",
};

/** The window keeps a thin line in DJ mode's pink along its top, as YOBUKO's windows do. */
export const DJ_MODE = "#DB2777";

export function parseHex(text: string): Rgb | null {
  const digits = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim())?.[1];
  if (!digits) return null;
  const d = digits.length === 3 ? digits.split("").map((c) => c + c).join("") : digits;
  return { r: parseInt(d.slice(0, 2), 16), g: parseInt(d.slice(2, 4), 16), b: parseInt(d.slice(4, 6), 16) };
}

export function toHex(c: Rgb): string {
  const h = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0").toUpperCase();
  return `#${h(c.r)}${h(c.g)}${h(c.b)}`;
}

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  const k = Math.min(1, Math.max(0, t));
  return { r: a.r + (b.r - a.r) * k, g: a.g + (b.g - a.g) * k, b: a.b + (b.b - a.b) * k };
}

const lightness = (c: Rgb) => (Math.max(c.r, c.g, c.b) + Math.min(c.r, c.g, c.b)) / 510;
const chroma = (c: Rgb) => (Math.max(c.r, c.g, c.b) - Math.min(c.r, c.g, c.b)) / 255;

/** One of rbxport's greys on the look's ramp. */
export function mapGrey(colour: Rgb, look: Look): Rgb {
  const read = (hex: string) => parseHex(hex) ?? { r: 0, g: 0, b: 0 };
  const ground = read(look.ground);
  const deep = mix(ground, look.dark ? { r: 0, g: 0, b: 0 } : { r: 255, g: 255, b: 255 }, 0.45);
  const ramp: [number, Rgb][] = [
    [0, deep],
    [0.05, mix(ground, deep, 0.5)],
    [0.09, ground],
    [0.14, read(look.surface)],
    [0.2, read(look.control)],
    [0.3, read(look.controlActive)],
    [0.45, read(look.hint)],
    [0.7, read(look.textDim)],
    [1, read(look.text)],
  ];
  const l = lightness(colour);
  for (let i = 1; i < ramp.length; i++) {
    const low = ramp[i - 1];
    const high = ramp[i];
    if (low && high && l <= high[0]) return mix(low[1], high[1], (l - low[0]) / (high[0] - low[0]));
  }
  return read(look.text);
}

/** The token values to set on the window for `look`, from rbxport's own `base` values. */
export function overrides(base: Record<string, string>, look: Look): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, value] of Object.entries(base)) {
    const colour = parseHex(value);
    if (!colour) continue;
    if ((ACCENT_TOKENS as readonly string[]).includes(name)) out[name] = look.accent;
    else if ((ON_ACCENT as readonly string[]).includes(name)) out[name] = look.inverse;
    else if (SEMANTIC[name]) out[name] = String(look[SEMANTIC[name]]);
    else if (chroma(colour) < 0.12) out[name] = toHex(mapGrey(colour, look));
  }
  out["--c-select"] = toHex(mix(parseHex(look.surface) ?? { r: 0, g: 0, b: 0 }, parseHex(look.accent) ?? { r: 0, g: 0, b: 0 }, 0.32));
  out["--yobuko-mode"] = DJ_MODE;
  return out;
}

/** rbxport's own colour tokens, as its style sheets set them on `:root`. */
function baseTokens(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    const walk = (list: CSSRuleList) => {
      for (const rule of Array.from(list)) {
        if (rule instanceof CSSStyleRule && rule.selectorText.split(",").some((s) => s.trim() === ":root")) {
          for (const name of Array.from(rule.style)) {
            if (name.startsWith("--c-")) out[name] = rule.style.getPropertyValue(name).trim();
          }
        } else if ("cssRules" in rule && (rule as CSSGroupingRule).cssRules) {
          walk((rule as CSSGroupingRule).cssRules);
        }
      }
    };
    walk(rules);
  }
  return out;
}

let base: Record<string, string> | null = null;
let applied = "";

/** Puts `text` (look.json) on the window; nothing when it is the look already shown or does not read. */
export function applyLook(text: string | null | undefined): boolean {
  if (!text || text === applied) return false;
  let look: Look;
  try {
    look = JSON.parse(text) as Look;
  } catch {
    return false;
  }
  if (!parseHex(look.ground ?? "") || !parseHex(look.text ?? "") || !parseHex(look.accent ?? "")) return false;
  base ??= baseTokens();
  const root = document.documentElement;
  for (const [name, value] of Object.entries(overrides(base, look))) root.style.setProperty(name, value);
  root.style.colorScheme = look.dark ? "dark" : "light";
  root.dataset.yobukoLook = look.dark ? "dark" : "light";
  applied = text;
  return true;
}

async function readLook(): Promise<string | null> {
  if (__RBL_MOCK_BACKEND__) {
    try {
      return window.localStorage.getItem("yobuko-look");
    } catch {
      return null;
    }
  }
  const { tauriInvoke } = await import("@/ipc/invoke");
  const invoke = await tauriInvoke();
  return invoke<string | null>("yobuko_look");
}

/** Takes YOBUKO's look now, then follows it as the studio's theme changes. */
export async function startYobukoLook(): Promise<void> {
  document.documentElement.style.setProperty("--yobuko-mode", DJ_MODE);
  applyLook(await readLook().catch(() => null));
  window.setInterval(() => {
    void readLook().then(applyLook).catch(() => {});
  }, 2000);
}

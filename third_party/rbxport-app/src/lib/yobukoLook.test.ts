import { describe, expect, it } from "vitest";
import { ACCENT_TOKENS, mapGrey, overrides, parseHex, type Look } from "./yobukoLook";

const ink: Look = {
  dark: true, ground: "#1D1C1A", surface: "#252421", control: "#343330", controlHover: "#3F3D3A",
  controlActive: "#4B4A46", lineStrong: "#484642", text: "#ECE8E1", textDim: "#B5B2AC", hint: "#85837E",
  inverse: "#1D1C1A", accent: "#A6A3FF", recording: "#E5484D", meterNormal: "#8BC48A", meterHot: "#E3C86A",
};
const washi: Look = { ...ink, dark: false, ground: "#F4EFE6", surface: "#E9E2D5", control: "#DCD4C6", controlActive: "#CFC6B6", text: "#2A2723", textDim: "#5A554D", hint: "#8A847A", accent: "#3F7FBF" };

describe("YOBUKO's look on rbxport", () => {
  it("moves greys onto the look's ramp, blues onto its accent, and leaves data colours alone", () => {
    const base = { "--c-text": "#E6E6E6", "--c-accent": "#1373EB", "--c-panel": "#1E1E1E", "--c-cue-pink": "#F51E8C", "--c-error": "#E23B2E", "--c-modal-scrim": "rgb(0 0 0 / 55%)" };
    const out = overrides(base, ink);
    expect(out["--c-accent"]).toBe(ink.accent);
    expect(out["--c-error"]).toBe(ink.recording);
    expect(out["--c-cue-pink"]).toBeUndefined();
    expect(out["--c-modal-scrim"]).toBeUndefined();
    expect(parseHex(out["--c-text"] ?? "")!.r).toBeGreaterThan(200);
    expect(ACCENT_TOKENS).toContain("--c-tab-active");
  });

  it("turns rbxport light under a light look", () => {
    const panel = mapGrey(parseHex("#1E1E1E")!, washi);
    const text = mapGrey(parseHex("#E6E6E6")!, washi);
    expect(panel.r).toBeGreaterThan(text.r);
  });
});

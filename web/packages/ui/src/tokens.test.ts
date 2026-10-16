import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";
import {
  accentVars,
  contrast,
  NEUTRAL_OKLCH,
  oklchToHex,
  rampOklch,
  SIGNALS,
  SURFACE_OKLCH,
  type Oklch,
} from "./colors.ts";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

/** The value of `--name` in the first block that declares it after `from`. */
function token(name: string, from = 0): string {
  const m = new RegExp(`--${name}:\\s*([^;]+);`).exec(css.slice(from));
  if (!m) throw new Error(`missing token --${name}`);
  return m[1]!.trim();
}
function oklch(v: string): Oklch {
  const m = /^oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)$/.exec(v);
  if (!m) throw new Error(`not an oklch literal: ${v}`);
  return { l: +m[1]!, c: +m[2]!, h: +m[3]! };
}

describe("tokens.css stays in step with colors.ts", () => {
  test("neutral ramp", () => {
    for (const [step, v] of Object.entries(NEUTRAL_OKLCH)) expect(token(`neutral-${step}`)).toBe(v);
  });

  test.each(Object.entries(SIGNALS))("%s ramp", (name, { hue, chroma }) => {
    for (const [step, v] of Object.entries(rampOklch(hue, chroma)))
      expect(token(`${name}-${step}`)).toBe(v);
  });

  test("default accent variables", () => {
    for (const [k, v] of Object.entries(accentVars("#6d3fd0")))
      if (k !== "--accent-h") expect(token(k.slice(2))).toBe(v);
  });

  test("surfaces", () => {
    const dark = css.indexOf(':root[data-theme="dark"]');
    for (const name of ["bg", "surface"] as const) {
      const key = name === "bg" ? "canvas" : "panel";
      const light = oklch(token(name));
      const lightWant = SURFACE_OKLCH.light[key];
      expect(light.l).toBeCloseTo(lightWant.l, 3);
      expect(oklch(token(name, dark)).l).toBeCloseTo(SURFACE_OKLCH.dark[key].l, 3);
    }
  });

  test("the dark theme is declared identically for data-theme and for the system preference", () => {
    const block = (start: string) => {
      const i = css.indexOf(start);
      const open = css.indexOf("{", i);
      let depth = 0;
      for (let j = open; j < css.length; j++) {
        if (css[j] === "{") depth++;
        if (css[j] === "}" && --depth === 0) return css.slice(open + 1, j);
      }
      return "";
    };
    const strip = (s: string) =>
      s
        .replace(/color-scheme:[^;]+;/, "")
        .split(";")
        .map((x) => x.replace(/\s+/g, " ").trim())
        .filter(Boolean);
    const explicit = strip(block(':root[data-theme="dark"] {\n  color-scheme'));
    const media = strip(block(':root:not([data-theme="light"]) {\n    --bg'));
    expect(media).toEqual(explicit);
  });
});

describe("semantic text and surface pairs meet WCAG AA", () => {
  const pair = (fg: string, bg: string) => contrast(oklchToHex(oklch(fg)), oklchToHex(oklch(bg)));
  for (const [mode, from] of [
    ["light", 0],
    ["dark", css.indexOf(':root[data-theme="dark"]')],
  ] as const) {
    test(`${mode}: text, muted and subtle text on every surface`, () => {
      for (const t of ["text", "text-muted", "text-subtle"])
        for (const s of ["bg", "surface", "surface-2", "surface-3"])
          expect(
            pair(token(t, from), token(s, from)),
            `${mode} ${t} on ${s}`,
          ).toBeGreaterThanOrEqual(4.5);
    });
    test(`${mode}: borders that identify controls reach 3:1 on the surface`, () => {
      // --border-control outlines inputs (WCAG 1.4.11); --border and --border-strong only separate.
      for (const s of ["surface", "surface-2"])
        expect(pair(token("border-control", from), token(s, from))).toBeGreaterThanOrEqual(3);
    });
  }
});

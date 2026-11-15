import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";
import { render, SCALES, SIGNAL_STEPS, SOFT_PERCENT } from "../scripts/generate-tokens.ts";
import { oklchToHex, SIGNALS, ramp } from "../../ui/src/colors.ts";
import { tokens } from "./tokens.native.ts";

const here = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(here, "../../ui/src/tokens.css"), "utf8");
const committed = readFileSync(resolve(here, "tokens.native.ts"), "utf8");

/** The text of the first block that starts with `selector {`. */
function block(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no block ${selector}`);
  let depth = 0;
  for (let i = css.indexOf("{", start); i < css.length; i++) {
    if (css[i] === "{") depth++;
    if (css[i] === "}" && --depth === 0) return css.slice(start, i + 1);
  }
  throw new Error(`unclosed block ${selector}`);
}
const blocks = { light: block(":root"), dark: block(':root[data-theme="dark"]') };

/** The raw value of `--name` in a block, or throws. */
function decl(text: string, name: string): string {
  const m = new RegExp(`--${name}:s*([^;]+);`).exec(text);
  if (!m) throw new Error(`missing token --${name}`);
  return m[1]!.trim();
}
function hexOf(value: string): string {
  const m = /^oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)$/.exec(value);
  if (!m) throw new Error(`not an oklch literal: ${value}`);
  return oklchToHex({ l: +m[1]!, c: +m[2]!, h: +m[3]! });
}
/** `--signal-600` lives in the primitives, which only the light block (:root) declares. */
const primitive = (name: string) => hexOf(decl(blocks.light, name));

describe("tokens.native.ts", () => {
  test("is what the generator writes (run `vp run generate:tokens` after a change)", () => {
    expect(committed).toBe(render());
  });

  describe.each(["light", "dark"] as const)("%s palette agrees with tokens.css", (mode) => {
    const css = blocks[mode];
    const t = tokens[mode];

    test("surfaces, borders and text", () => {
      expect(t.bg).toBe(hexOf(decl(css, "bg")));
      expect(t.surface).toBe(hexOf(decl(css, "surface")));
      for (const [key, name] of [
        ["surface2", "surface-2"],
        ["surface3", "surface-3"],
        ["border", "border"],
        ["borderStrong", "border-strong"],
        ["borderControl", "border-control"],
        ["text", "text"],
        ["textMuted", "text-muted"],
        ["textSubtle", "text-subtle"],
      ] as const)
        expect(t[key], key).toBe(hexOf(decl(css, name)));
    });

    test("signal fills and inks are the ramp steps the CSS names", () => {
      const steps = SIGNAL_STEPS[mode];
      for (const name of Object.keys(SIGNALS) as (keyof typeof SIGNALS)[]) {
        expect(decl(css, name)).toBe(`var(--${name}-${steps.fill})`);
        expect(decl(css, `${name}-ink`)).toBe(`var(--${name}-${steps.ink})`);
        expect(t[name].fill, `${name} fill`).toBe(primitive(`${name}-${steps.fill}`));
        expect(t[name].ink, `${name} ink`).toBe(primitive(`${name}-${steps.ink}`));
        expect(t[name].fill).toBe(ramp(SIGNALS[name].hue, SIGNALS[name].chroma)[steps.fill]);
      }
    });

    test("accent fill, ink and the text on it", () => {
      const suffix = mode === "light" ? "l" : "d";
      expect(t.accentFill).toBe(decl(blocks.light, `accent-fill-${suffix}`));
      expect(t.accentInk).toBe(decl(blocks.light, `accent-ink-${suffix}`));
      expect(t.onAccent).toBe(decl(blocks.light, `on-accent-${suffix}`));
      expect(t.onDanger).toBe(decl(css, "on-danger"));
    });
  });

  test("the share of a signal in its soft tint is the CSS one", () => {
    for (const [name, pct] of Object.entries(SOFT_PERCENT))
      expect(css).toContain(
        `--${name}-soft: color-mix(in oklab, var(--${name}) ${pct}%, var(--surface));`,
      );
    expect(css).toContain(
      "--accent-soft: color-mix(in oklab, var(--accent-fill) 12%, var(--surface));",
    );
  });

  test("the default accent is the one of tokens.css", () => {
    expect(tokens.defaultAccent).toBe(decl(blocks.light, "accent"));
  });

  test("spacing, radius and type scales are tokens.css converted from rem to dp", () => {
    for (const [step, dp] of Object.entries(SCALES.space))
      expect(parseFloat(decl(blocks.light, `space-${step}`)) * 16, `space-${step}`).toBe(dp);
    expect(parseFloat(decl(blocks.light, "radius"))).toBe(SCALES.radius.lg);
    expect(SCALES.radius.sm).toBe(Math.max(4, SCALES.radius.lg * 0.5));
    expect(SCALES.radius.md).toBeCloseTo(Math.max(5, SCALES.radius.lg * 0.7), 5);
    expect(SCALES.radius.xl).toBe(SCALES.radius.lg * 1.5);
    for (const key of ["xs", "sm", "md", "lg", "xl", "2xl", "3xl", "4xl", "5xl"] as const)
      expect(parseFloat(decl(blocks.light, `text-${key}`)) * 16, `text-${key}`).toBeCloseTo(
        SCALES.fontSize[key],
        3,
      );
  });

  test("touch targets are at least 48 dp", () => {
    expect(SCALES.control.height).toBeGreaterThanOrEqual(48);
    expect(SCALES.control.minTouch).toBeGreaterThanOrEqual(48);
    expect(SCALES.control.heightLg).toBeGreaterThanOrEqual(SCALES.control.height);
  });
});

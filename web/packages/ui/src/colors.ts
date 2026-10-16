/**
 * Colour maths for the obop design system: sRGB hex <-> OKLCH, WCAG contrast, accent ramps and the
 * automatic AA check behind the theme picker. Pure functions, no DOM, so they are unit-tested.
 */

export type Oklch = { l: number; c: number; h: number };
type Rgb = [number, number, number];

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const fromLinear = (v: number) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);

/** `#rgb` or `#rrggbb` to 0..1 sRGB channels; anything else is `null`. */
export function parseHex(hex: string): Rgb | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let h = m[1]!;
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  const n = parseInt(h, 16);
  return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

export function toHex([r, g, b]: Rgb): string {
  const x = (v: number) =>
    Math.round(clamp01(v) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${x(r)}${x(g)}${x(b)}`;
}

function rgbToOklab([r, g, b]: Rgb): [number, number, number] {
  const lr = toLinear(r);
  const lg = toLinear(g);
  const lb = toLinear(b);
  const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
  const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
  const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function oklabToLinear([L, a, b]: [number, number, number]): Rgb {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

export function hexToOklch(hex: string): Oklch {
  const rgb = parseHex(hex) ?? [0, 0, 0];
  const [L, a, b] = rgbToOklab(rgb);
  const c = Math.hypot(a, b);
  return { l: L, c, h: c < 1e-4 ? 0 : ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360 };
}

const inGamut = (rgb: Rgb) => rgb.every((v) => v >= -0.0005 && v <= 1.0005);

/** OKLCH to hex. Out-of-gamut colours lose chroma (not lightness) until they fit. */
export function oklchToHex({ l, c, h }: Oklch): string {
  const rad = (h * Math.PI) / 180;
  let chroma = c;
  for (let i = 0; i < 40; i++) {
    const lin = oklabToLinear([l, chroma * Math.cos(rad), chroma * Math.sin(rad)]);
    if (inGamut(lin) || chroma < 1e-4) return toHex(lin.map(fromLinear) as Rgb);
    chroma *= 0.94;
  }
  return toHex([l, l, l]);
}

/** WCAG 2 relative luminance. */
export function luminance(hex: string): number {
  const [r, g, b] = (parseHex(hex) ?? [0, 0, 0]).map(toLinear) as Rgb;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG 2 contrast ratio, 1 to 21. */
export function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** `color-mix(in oklab, a pct%, b)` as a hex colour, so JS and CSS agree on tints. */
export function mixOklab(a: string, b: string, pct: number): string {
  const [l1, a1, b1] = rgbToOklab(parseHex(a) ?? [0, 0, 0]);
  const [l2, a2, b2] = rgbToOklab(parseHex(b) ?? [0, 0, 0]);
  const t = pct / 100;
  const lab: [number, number, number] = [
    l1 * t + l2 * (1 - t),
    a1 * t + a2 * (1 - t),
    b1 * t + b2 * (1 - t),
  ];
  return toHex(oklabToLinear(lab).map(fromLinear) as Rgb);
}

/* ---------- Foundations: the values the tokens compile to ---------- */

export const RAMP_STEPS = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const;
export type RampStep = (typeof RAMP_STEPS)[number];

/** Lightness per step, shared by every ramp so steps mean the same thing in every hue. */
export const RAMP_L = [0.972, 0.945, 0.89, 0.825, 0.745, 0.66, 0.57, 0.485, 0.4, 0.32, 0.235];
const RAMP_C = [0.1, 0.22, 0.42, 0.62, 0.82, 0.98, 1, 0.9, 0.78, 0.64, 0.5];

/** An 11-step ramp (50 to 950) for one hue; `chroma` is the peak chroma at 500 to 600. */
export function ramp(hue: number, chroma: number): Record<RampStep, string> {
  const out = {} as Record<RampStep, string>;
  RAMP_STEPS.forEach((s, i) => {
    out[s] = oklchToHex({ l: RAMP_L[i]!, c: chroma * RAMP_C[i]!, h: hue });
  });
  return out;
}

/** Neutral cool grey: the only ramp with a fixed hue, the rest follow the accent or the signal. */
export const NEUTRAL_HUE = 265;
/** Neutrals keep a whisper of chroma at every step, which is what makes the greys read as cool. */
const NEUTRAL_C = [0.004, 0.006, 0.01, 0.013, 0.017, 0.02, 0.022, 0.022, 0.02, 0.017, 0.013];
export const neutral = Object.fromEntries(
  RAMP_STEPS.map((s, i) => [s, oklchToHex({ l: RAMP_L[i]!, c: NEUTRAL_C[i]!, h: NEUTRAL_HUE })]),
) as Record<RampStep, string>;

/** The ramp as `oklch()` strings, the form `tokens.css` stores. */
export function rampOklch(hue: number, chroma: number | number[]): Record<RampStep, string> {
  const out = {} as Record<RampStep, string>;
  RAMP_STEPS.forEach((s, i) => {
    const c = typeof chroma === "number" ? chroma * RAMP_C[i]! : chroma[i]!;
    out[s] = `oklch(${RAMP_L[i]} ${+c.toFixed(4)} ${hue})`;
  });
  return out;
}
export const NEUTRAL_OKLCH = rampOklch(NEUTRAL_HUE, NEUTRAL_C);

/** Signal ramps: hue and peak chroma. */
export const SIGNALS = {
  success: { hue: 152, chroma: 0.16 },
  warning: { hue: 78, chroma: 0.15 },
  danger: { hue: 22, chroma: 0.2 },
  info: { hue: 245, chroma: 0.15 },
} as const;
export type Signal = keyof typeof SIGNALS;

/**
 * The surfaces text and controls sit on, per mode. These mirror `tokens.css` (a unit test keeps the
 * two in step); the contrast check below measures against them.
 */
export const SURFACE_OKLCH = {
  light: { canvas: { l: 0.962, c: 0.005, h: 265 }, panel: { l: 1, c: 0, h: 265 } },
  dark: { canvas: { l: 0.155, c: 0.012, h: 265 }, panel: { l: 0.2, c: 0.013, h: 265 } },
} as const;
export const SURFACES = {
  light: {
    canvas: oklchToHex(SURFACE_OKLCH.light.canvas),
    panel: oklchToHex(SURFACE_OKLCH.light.panel),
  },
  dark: {
    canvas: oklchToHex(SURFACE_OKLCH.dark.canvas),
    panel: oklchToHex(SURFACE_OKLCH.dark.panel),
  },
} as const;
export type Mode = keyof typeof SURFACES;

/* ---------- The accent and its automatic AA check ---------- */

export const WHITE = "#ffffff";
export const NEAR_BLACK = "#0c0d12";
export const AA_TEXT = 4.5;
export const AA_UI = 3;

export type Check = { id: string; label: string; ratio: number; min: number; pass: boolean };
export type ModeAccent = {
  /** Button and selected-control fill. */
  fill: string;
  /** Text on the fill. */
  onFill: string;
  /** Links, active labels and icons on a panel or the canvas. */
  ink: string;
  checks: Check[];
};
export type Accent = {
  /** What the person picked. */
  chosen: string;
  /** True when light mode needed a different step than the colour the person picked. */
  adjusted: boolean;
  /** True when dark mode uses a lighter step than the pick (the usual case for dark colours). */
  darkShifted: boolean;
  light: ModeAccent;
  dark: ModeAccent;
  ramp: Record<RampStep, string>;
};

const bestOn = (bg: string) =>
  contrast(WHITE, bg) >= contrast(NEAR_BLACK, bg) ? WHITE : NEAR_BLACK;

/** Move lightness by `dir` until `ok` passes (or the end of the range). */
function search(from: Oklch, dir: 1 | -1, ok: (hex: string) => boolean): string {
  let { l } = from;
  for (let i = 0; i < 120; i++) {
    const hex = oklchToHex({ ...from, l });
    if (ok(hex)) return hex;
    l = clamp01(l + dir * 0.005);
  }
  return oklchToHex({ ...from, l });
}

function resolveMode(chosen: string, mode: Mode): ModeAccent {
  const { canvas, panel } = SURFACES[mode];
  const lch = hexToOklch(chosen);
  const dir = mode === "light" ? -1 : 1;
  const soft = (c: string) => mixOklab(c, panel, 12);
  // Fill: readable text on it, and visible against both surfaces (3:1).
  const fill = search(
    mode === "dark" ? { ...lch, l: Math.max(lch.l, 0.62) } : lch,
    dir,
    (c) =>
      contrast(bestOn(c), c) >= AA_TEXT &&
      contrast(c, panel) >= AA_UI &&
      contrast(c, canvas) >= AA_UI,
  );
  // Ink: text on the panel, the canvas and the 12% tint used for selected rows.
  const ink = search(
    hexToOklch(fill),
    dir,
    (c) =>
      contrast(c, panel) >= AA_TEXT &&
      contrast(c, canvas) >= AA_TEXT &&
      contrast(c, soft(fill)) >= AA_TEXT,
  );
  const onFill = bestOn(fill);
  const checks: Check[] = [
    {
      id: "on-fill",
      label: "Text on the accent fill",
      ratio: contrast(onFill, fill),
      min: AA_TEXT,
    },
    {
      id: "fill-panel",
      label: "Accent fill against the panel",
      ratio: contrast(fill, panel),
      min: AA_UI,
    },
    {
      id: "fill-canvas",
      label: "Accent fill against the canvas",
      ratio: contrast(fill, canvas),
      min: AA_UI,
    },
    {
      id: "ink-panel",
      label: "Accent text on the panel",
      ratio: contrast(ink, panel),
      min: AA_TEXT,
    },
    {
      id: "ink-canvas",
      label: "Accent text on the canvas",
      ratio: contrast(ink, canvas),
      min: AA_TEXT,
    },
    {
      id: "ink-tint",
      label: "Accent text on a selected row",
      ratio: contrast(ink, soft(fill)),
      min: AA_TEXT,
    },
  ].map((c) => ({ ...c, pass: c.ratio >= c.min }));
  return { fill, onFill, ink, checks };
}

export function resolveAccent(chosen: string): Accent {
  const hex = parseHex(chosen) ? toHex(parseHex(chosen)!) : "#6d3fd0";
  const lch = hexToOklch(hex);
  const light = resolveMode(hex, "light");
  const dark = resolveMode(hex, "dark");
  const same = (a: string) => a.toLowerCase() === hex.toLowerCase();
  return {
    chosen: hex,
    adjusted: !same(light.fill),
    darkShifted: !same(dark.fill),
    light,
    dark,
    ramp: ramp(lch.h, Math.min(Math.max(lch.c, 0.06), 0.24)),
  };
}

/** The custom properties that carry an accent into the document. */
export function accentVars(chosen: string): Record<string, string> {
  const a = resolveAccent(chosen);
  const v: Record<string, string> = {
    "--accent": a.chosen,
    "--accent-fill-l": a.light.fill,
    "--accent-fill-d": a.dark.fill,
    "--accent-ink-l": a.light.ink,
    "--accent-ink-d": a.dark.ink,
    "--on-accent-l": a.light.onFill,
    "--on-accent-d": a.dark.onFill,
  };
  for (const s of RAMP_STEPS) v[`--accent-${s}`] = a.ramp[s];
  return v;
}

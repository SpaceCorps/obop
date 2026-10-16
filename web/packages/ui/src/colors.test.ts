import { describe, expect, test } from "vite-plus/test";
import {
  contrast,
  hexToOklch,
  luminance,
  mixOklab,
  oklchToHex,
  resolveAccent,
  ramp,
  RAMP_STEPS,
} from "./colors.ts";

describe("colour maths", () => {
  test("hex round-trips through OKLCH", () => {
    for (const hex of ["#6d3fd0", "#0b8a8f", "#d9730d", "#495670", "#ffffff", "#000000"]) {
      expect(oklchToHex(hexToOklch(hex))).toBe(hex);
    }
  });

  test("contrast matches the WCAG reference values", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
    expect(luminance("#ffffff")).toBeCloseTo(1, 5);
  });

  test("a 50% oklab mix sits between its inputs", () => {
    const mid = hexToOklch(mixOklab("#000000", "#ffffff", 50));
    expect(mid.l).toBeGreaterThan(0.4);
    expect(mid.l).toBeLessThan(0.8);
  });

  test("ramps get darker from 50 to 950", () => {
    const r = ramp(260, 0.15);
    const l = RAMP_STEPS.map((s) => hexToOklch(r[s]).l);
    for (let i = 1; i < l.length; i++) expect(l[i]!).toBeLessThan(l[i - 1]!);
  });
});

describe("the accent check", () => {
  const picks = ["#6d3fd0", "#3b5bdb", "#0b8a8f", "#2b8a3e", "#d9730d", "#d6336c", "#495670"];

  test.each(picks)("every preset passes AA in both modes: %s", (hex) => {
    const a = resolveAccent(hex);
    for (const mode of [a.light, a.dark])
      for (const c of mode.checks)
        expect(c.pass, `${hex} ${c.label} ${c.ratio.toFixed(2)}`).toBe(true);
  });

  test.each(["#ffee00", "#ffffff", "#000000", "#aaff00", "#101010"])(
    "extreme colours are corrected until they pass: %s",
    (hex) => {
      const a = resolveAccent(hex);
      expect(a.adjusted || a.darkShifted).toBe(true);
      for (const mode of [a.light, a.dark])
        for (const c of mode.checks)
          expect(c.pass, `${hex} ${c.label} ${c.ratio.toFixed(2)}`).toBe(true);
    },
  );

  test("a colour that already passes is kept as chosen", () => {
    const a = resolveAccent("#6d3fd0");
    expect(a.light.fill).toBe("#6d3fd0");
  });
});

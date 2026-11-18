import { EARLY_FIX_MS, distanceMeters } from "@obop/trip-core";
import { describe, expect, test } from "vite-plus/test";
import { RECORDING_THINNING, acceptFixes, keepFix, type Anchor } from "./thin.ts";
import type { NewFix } from "./types.ts";

const T0 = Date.parse("2026-11-18T09:00:00Z");
const { minGapMs, farM } = RECORDING_THINNING;

/** About 0.00001 degrees of latitude are 1.11 m. */
const north = (metres: number) => 59 + metres / 111_195;
const here = (t: number, metres = 0): Anchor => ({ t, lat: north(metres), lon: 18 });
const fixAt = (t: number, metres = 0): NewFix => ({
  ...here(t, metres),
  accuracyM: 5,
  speedMps: null,
});

/** `n` fixes `everyMs` apart, moving north at `mps`, from `T0 + fromMs`. */
function drive(n: number, everyMs: number, mps: number, fromMs = 0): NewFix[] {
  return Array.from({ length: n }, (_, i) => {
    const t = T0 + fromMs + i * everyMs;
    return fixAt(t, ((t - T0) / 1000) * mps);
  });
}

describe("which fix is kept after the last stored one", () => {
  test("the first fix of a trip is kept", () => {
    expect(keepFix(null, here(T0), RECORDING_THINNING)).toBe(true);
  });

  test("a fix that comes soon after the last one and close to it is dropped", () => {
    expect(keepFix(here(T0), here(T0 + 1000, 14), RECORDING_THINNING)).toBe(false);
    expect(keepFix(here(T0), here(T0 + minGapMs - 1, 0), RECORDING_THINNING)).toBe(false);
  });

  test("a fix at the minimum gap, or later, is kept however close it is", () => {
    expect(keepFix(here(T0), here(T0 + minGapMs, 0), RECORDING_THINNING)).toBe(true);
    expect(keepFix(here(T0), here(T0 + 5000, 0), RECORDING_THINNING)).toBe(true);
  });

  test("a fix that is far from the last one is kept even when it comes soon", () => {
    expect(keepFix(here(T0), here(T0 + 500, farM + 1), RECORDING_THINNING)).toBe(true);
    expect(keepFix(here(T0), here(T0 + 500, farM - 5), RECORDING_THINNING)).toBe(false);
  });

  test("a fix exactly as far as the limit is kept", () => {
    const a = here(T0);
    const b = here(T0 + 500, 30);
    const exactly = { minGapMs: 10_000, farM: distanceMeters(a, b) };
    expect(keepFix(a, b, exactly)).toBe(true);
    expect(keepFix(a, b, { ...exactly, farM: exactly.farM + 0.001 })).toBe(false);
  });
});

describe("the fixes of one delivery that are stored", () => {
  test("iOS delivers a fix every second: about every second one is stored", () => {
    const kept = acceptFixes(null, T0, drive(60, 1000, 14));
    expect(kept.length).toBe(30);
    expect(kept.every((f, i) => i === 0 || f.t - (kept[i - 1]?.t ?? 0) >= minGapMs)).toBe(true);
  });

  test("Android delivers a fix about every five seconds: all of them are stored", () => {
    expect(acceptFixes(null, T0, drive(12, 5000, 14)).length).toBe(12);
  });

  test("a fix that jumped far in a second is kept", () => {
    const kept = acceptFixes(null, T0, [fixAt(T0), fixAt(T0 + 1000, farM + 10)]);
    expect(kept.length).toBe(2);
  });

  test("the last stored fix of an earlier delivery counts", () => {
    const last = here(T0 + 10_000, 140);
    const kept = acceptFixes(last, T0, drive(3, 1000, 14, 11_000));
    // 11 s is too soon after the stored 10 s, 12 s is 2 s after it and is kept, 13 s is too soon
    // after 12 s. Each step is 14 m, below the 50 m that would keep a fix anyway.
    expect(kept.map((f) => f.t - T0)).toEqual([12_000]);
  });

  test("a batch out of order is sorted first, and a repeated fix is stored once", () => {
    const a = fixAt(T0, 0);
    const b = fixAt(T0 + 5000, 70);
    const c = fixAt(T0 + 10_000, 140);
    const kept = acceptFixes(null, T0, [c, a, b, b]);
    expect(kept).toEqual([a, b, c]);
  });

  test("a fix not later than the last stored one is dropped", () => {
    const last = here(T0 + 5000, 70);
    expect(acceptFixes(last, T0, [fixAt(T0 + 5000, 70), fixAt(T0 + 4000, 56)])).toEqual([]);
  });

  test("a cached position from well before Start is dropped, a fix just before it is kept", () => {
    const stale = fixAt(T0 - EARLY_FIX_MS - 1, 0);
    const justBefore = fixAt(T0 - 3000, 0);
    expect(acceptFixes(null, T0, [stale, justBefore])).toEqual([justBefore]);
  });

  test("a fix that is not plausible is dropped and does not become the anchor", () => {
    const bad: NewFix = { ...fixAt(T0 + 1000, 0), accuracyM: -1 };
    const good = fixAt(T0 + 1500, 0);
    expect(acceptFixes(null, T0, [bad, good])).toEqual([good]);
  });

  test("nothing delivered, nothing stored", () => {
    expect(acceptFixes(null, T0, [])).toEqual([]);
  });
});

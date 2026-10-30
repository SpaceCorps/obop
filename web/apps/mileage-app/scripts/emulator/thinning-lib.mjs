// The pure part of thinning-replay.mjs: the thinning rule of the app (`src/core/thin.ts`, the same
// `keepFix` that `appendFixes` calls at write time) run over a track that was recorded WITHOUT
// thinning, so that one drive can be judged under several rules.
import { distanceMeters } from "@obop/trip-core";
import { keepFix } from "../../src/core/thin.ts";

/** The rules the replay compares. The first is the app's default (`RECORDING_THINNING`). */
export const RULES = [
  { name: "2 s / 50 m (the default)", thinning: { minGapMs: 2000, farM: 50 } },
  { name: "1.5 s / 50 m", thinning: { minGapMs: 1500, farM: 50 } },
  { name: "off", thinning: null },
];

/**
 * Fixes from the rows of `mileage_point` (`recorded_at`, `lat`, `lon`, as the REST API gives them)
 * or from objects that already have `t` in milliseconds.
 * @param {Array<Record<string, unknown>>} rows
 * @returns {Array<{ t: number, lat: number, lon: number }>}
 */
export function toFixes(rows) {
  return rows.map((row) => ({
    t: typeof row.t === "number" ? row.t : Date.parse(String(row.recorded_at)),
    lat: Number(row.lat),
    lon: Number(row.lon),
  }));
}

/**
 * What `appendFixes` would have stored from these fixes under `thinning` (`null` is no thinning):
 * a fix is dropped when it is not later than the last stored one, or when `keepFix` says so.
 * @param {Array<{ t: number, lat: number, lon: number }>} fixes
 * @param {{ minGapMs: number, farM: number } | null} thinning
 */
export function replay(fixes, thinning) {
  const ordered = [...fixes].sort((a, b) => a.t - b.t);
  let anchor = null;
  let metres = 0;
  let kept = 0;
  for (const fix of ordered) {
    if (anchor !== null && fix.t <= anchor.t) continue;
    if (thinning !== null && !keepFix(anchor, fix, thinning)) continue;
    if (anchor !== null) metres += distanceMeters(anchor, fix);
    anchor = fix;
    kept += 1;
  }
  return { kept, distanceM: metres };
}

/**
 * How the track arrived: the number of fixes, the time they span and the gaps between neighbours.
 * @param {Array<{ t: number }>} fixes
 */
export function cadence(fixes) {
  const times = fixes.map((f) => f.t).sort((a, b) => a - b);
  if (times.length < 2) return { count: times.length, spanS: 0, perMinute: 0, gapsS: null };
  const gaps = times.slice(1).map((t, i) => (t - times[i]) / 1000);
  const sorted = [...gaps].sort((a, b) => a - b);
  const spanS = (times.at(-1) - times[0]) / 1000;
  return {
    count: times.length,
    spanS,
    perMinute: spanS > 0 ? (times.length - 1) / (spanS / 60) : 0,
    gapsS: {
      smallest: sorted[0],
      median: sorted[Math.floor(sorted.length / 2)],
      largest: sorted.at(-1),
    },
  };
}

/** Whether the track looks as if the app thinned it already (no two neighbours closer than the default gap). */
export function looksThinned(fixes) {
  const c = cadence(fixes);
  return c.gapsS !== null && c.count >= 10 && c.gapsS.smallest >= 2;
}

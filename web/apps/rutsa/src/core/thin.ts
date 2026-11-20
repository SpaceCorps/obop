import { EARLY_FIX_MS, distanceMeters } from "@obop/trip-core";
import { plausible } from "./fix.ts";
import type { NewFix } from "./types.ts";

/**
 * Thinning of fixes at write time. Android delivers a fix about every 5 seconds (the `timeInterval`
 * of the recorder); iOS has no such option and, with a distance filter of 5 m, a moving car gets
 * about one fix per second. The same drive should leave a comparable number of points on both, so a
 * fix that comes too soon after the last stored one is not stored, unless the phone has moved far.
 * The numbers are those of `mileage-app` (Proposed there, set by its Simulator and iPhone runs).
 */
export interface Thinning {
  /** A fix less than this long after the last stored one is dropped ... */
  minGapMs: number;
  /** ... unless it is at least this far from it (metres). */
  farM: number;
}

export const RECORDING_THINNING: Thinning = { minGapMs: 2000, farM: 50 };

/** The last stored fix of a trip. */
export interface Anchor {
  t: number;
  lat: number;
  lon: number;
}

/** Whether `fix` is stored, given the last stored fix (`null` when the trip has none yet). */
export function keepFix(last: Anchor | null, fix: Anchor, thinning: Thinning): boolean {
  if (last === null) return true;
  if (fix.t - last.t >= thinning.minGapMs) return true;
  return distanceMeters(last, fix) >= thinning.farM;
}

/**
 * The fixes of one delivery that are stored, in time order. The location service can deliver a
 * batch out of order, repeat a fix, or hand over a cached position from before Start or Resume (on
 * Resume often the place where the trip paused), so a fix is dropped when it is not plausible, when
 * it is not later than the last stored one, when it is older than the start of the segment by more
 * than `EARLY_FIX_MS` (the rule `analyseTrack` of `@obop/trip-core` applies too), or when the
 * thinning drops it.
 *
 * `last` is the last stored fix of the trip, read in the same transaction as the write (`null` when
 * the trip has none yet); `since` is the start of the trip's current segment in milliseconds: the
 * start of the trip, or of the last Resume.
 *
 * In `mileage-app` this is part of `appendFixes` of its store; here it is pure, so the store only
 * writes what it returns.
 */
export function acceptFixes(
  last: Anchor | null,
  since: number,
  incoming: readonly NewFix[],
  thinning: Thinning = RECORDING_THINNING,
): NewFix[] {
  const notBefore = since - EARLY_FIX_MS;
  let anchor = last;
  const kept: NewFix[] = [];
  const ordered = [...incoming].sort((a, b) => a.t - b.t);
  for (const fix of ordered) {
    if (!plausible(fix) || fix.t < notBefore) continue;
    if (anchor !== null && fix.t <= anchor.t) continue;
    if (!keepFix(anchor, fix, thinning)) continue;
    kept.push(fix);
    anchor = { t: fix.t, lat: fix.lat, lon: fix.lon };
  }
  return kept;
}

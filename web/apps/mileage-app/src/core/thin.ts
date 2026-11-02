import { distanceMeters } from "@obop/trip-core";

/**
 * Thinning of fixes at write time. Android delivers a fix about every 5 seconds (the `timeInterval`
 * of the recorder); iOS has no such option and, with a distance filter of 5 m, a moving car gets
 * about one fix per second. The same drive should leave a comparable number of points on both, so a
 * fix that comes too soon after the last stored one is not stored, unless the phone has moved far.
 * The numbers are Proposed: they are fixed by the Simulator and the iPhone runs (docs/spec/mobile.md
 * 8.8), and they leave the 5 second fixes of Android alone.
 */
export interface Thinning {
  /** A fix less than this long after the last stored one is dropped ... */
  minGapMs: number;
  /** ... unless it is at least this far from it (metres). */
  farM: number;
}

export const RECORDING_THINNING: Thinning = { minGapMs: 2000, farM: 50 };

/**
 * The thinning of a build. A TEST build made with `OBOP_RECORD_THINNING=off` (`extra.thinningOff`)
 * does not thin the fixes the phone delivers (`appendFixes` still drops an implausible fix, one not
 * later than the last stored and one older than the trip start by more than 10 s, and stops at
 * 20,000 points), so that one drive shows the cadence of the system and the rules can be replayed
 * over it (`scripts/emulator/thinning-replay.mjs`). Only a build that also allows plain http is a
 * test build (`extra.cleartextAllowed`); a release build keeps the rule whatever its configuration
 * says, and the release gate fails a release build that carries the variable
 * (`scripts/ios/gate.mjs`).
 */
export function thinningFor(extra: Record<string, unknown> | undefined | null): Thinning | null {
  return extra?.thinningOff === true && extra?.cleartextAllowed === true
    ? null
    : RECORDING_THINNING;
}

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

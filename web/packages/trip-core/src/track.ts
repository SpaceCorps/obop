import { distanceMeters } from "./distance.ts";

/** A step above this speed in metres per second (about 252 km/h) is not a real step (docs/spec/mobile.md 5.4). */
export const MAX_SPEED_MPS = 70;

/** A fix with an accuracy radius above this many metres is dropped. Proposed; set after the field test. */
export const MAX_ACCURACY_M = 100;

/** One recorded position. `t` is milliseconds since the Unix epoch. */
export interface Fix {
  readonly seq: number;
  readonly t: number;
  readonly lat: number;
  readonly lon: number;
  /** Accuracy radius in metres; absent when the platform did not say. */
  readonly accuracyM?: number | null;
}

export interface TrackOptions {
  readonly maxSpeedMps?: number;
  readonly maxAccuracyM?: number;
}

export interface TrackAnalysis {
  /** The summed distance of the accepted steps, in metres, not rounded. */
  readonly distanceM: number;
  /** The fixes that count. */
  readonly accepted: number;
  /** Fixes dropped because their accuracy radius was above the limit. */
  readonly skippedAccuracy: number;
  /** Fixes dropped because the step from the last accepted fix was too fast. */
  readonly skippedSpeed: number;
  /** Sequence numbers missing between the first and the last stored fix. */
  readonly seqGaps: number;
  /** The first accepted fix, `null` when there is none. */
  readonly first: Fix | null;
  /** The last accepted fix (the end position of the trip), `null` when there is none. */
  readonly last: Fix | null;
}

/**
 * The distance of a recorded track, the way the server computes it (docs/spec/mobile.md 5.4).
 * The server's figure is the one that counts; the device uses this one for the preview, and a
 * file of test vectors that both sides run keeps the two from drifting apart
 * (`track.fixtures.json`, read by `crates/modules/mileage` as well).
 *
 * Fixes are taken in order of `seq`. A fix whose accuracy radius is above `maxAccuracyM` is
 * dropped. Each further fix is compared with the last accepted one: if the step would need a
 * speed above `maxSpeedMps` (or goes back in time by any distance) it is dropped and the next fix
 * is compared with the same last accepted fix again, so a real move after a gap in the signal
 * is accepted once enough time has passed. A step of zero metres is accepted at any speed.
 */
export function analyseTrack(fixes: readonly Fix[], options: TrackOptions = {}): TrackAnalysis {
  const maxSpeed = options.maxSpeedMps ?? MAX_SPEED_MPS;
  const maxAccuracy = options.maxAccuracyM ?? MAX_ACCURACY_M;
  const sorted = [...fixes].sort((a, b) => a.seq - b.seq);
  let seqGaps = 0;
  for (let i = 1; i < sorted.length; i++) {
    const gap = sorted[i]!.seq - sorted[i - 1]!.seq - 1;
    if (gap > 0) seqGaps += gap;
  }
  let distanceM = 0;
  let accepted = 0;
  let skippedAccuracy = 0;
  let skippedSpeed = 0;
  let first: Fix | null = null;
  let last: Fix | null = null;
  for (const p of sorted) {
    if (p.accuracyM != null && p.accuracyM > maxAccuracy) {
      skippedAccuracy++;
      continue;
    }
    if (last !== null) {
      const d = distanceMeters(last, p);
      const dt = (p.t - last.t) / 1000;
      const plausible = dt > 0 ? d / dt <= maxSpeed : d === 0;
      if (!plausible) {
        skippedSpeed++;
        continue;
      }
      distanceM += d;
    } else {
      first = p;
    }
    accepted++;
    last = p;
  }
  return { distanceM, accepted, skippedAccuracy, skippedSpeed, seqGaps, first, last };
}

/** The distance as the server stores it: metres with two decimals, as text. */
export function distanceText(distanceM: number): string {
  return distanceM.toFixed(2);
}

/**
 * The integrity flags the server stores for a trip, in a fixed order, separated by spaces:
 * `seq_gap:N speed_jump:N low_accuracy:N`. A count of zero leaves its flag out; a clean track has
 * the empty string.
 */
export function integrityFlags(a: TrackAnalysis): string {
  const flags: string[] = [];
  if (a.seqGaps > 0) flags.push(`seq_gap:${a.seqGaps}`);
  if (a.skippedSpeed > 0) flags.push(`speed_jump:${a.skippedSpeed}`);
  if (a.skippedAccuracy > 0) flags.push(`low_accuracy:${a.skippedAccuracy}`);
  return flags.join(" ");
}

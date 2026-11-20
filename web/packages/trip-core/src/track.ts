import { distanceMeters } from "./distance.ts";

/** A step above this speed in metres per second (about 252 km/h) is not a real step (docs/spec/mobile.md 5.4). */
export const MAX_SPEED_MPS = 70;

/** A fix with an accuracy radius above this many metres is dropped. Proposed; set after the field test. */
export const MAX_ACCURACY_M = 100;

/**
 * A fix may be older than the start of the trip by this much and still belong to it (a fix made
 * just before the tap); an older one is a position from before the trip and is dropped.
 */
export const EARLY_FIX_MS = 10_000;

/**
 * This many fixes in a row that agree with each other (each step within the speed cap) and not
 * with the last accepted fix move the track to them: the track re-anchors (docs/spec/mobile.md
 * 5.4). One far-off fix, the first one included, then cannot ruin the distance of a whole trip.
 */
export const REANCHOR_RUN = 3;

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
  /** The start of the trip (ms since the Unix epoch): a fix more than `EARLY_FIX_MS` older is dropped. */
  readonly startedAt?: number | null;
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
  /** Fixes dropped because they were recorded more than `EARLY_FIX_MS` before the start. */
  readonly skippedEarly: number;
  /** How many times a run of fixes moved the track away from the last accepted fix. */
  readonly reanchors: number;
  /** Sequence numbers missing between the first and the last stored fix. */
  readonly seqGaps: number;
  /** The first accepted fix, `null` when there is none. */
  readonly first: Fix | null;
  /** The last accepted fix (the end position of the trip), `null` when there is none. */
  readonly last: Fix | null;
  /**
   * The route the distance runs through, as the `seq` of the fixes that count, in stretches: a
   * re-anchor after something was driven starts a new stretch (the jump between two stretches is
   * not part of the distance and is not a line of the route); a re-anchor before anything was
   * driven drops the fixes before it, because the trip starts where the run does. For a drawing
   * of the route (the back office's trip review, docs/ISSUES.md UB10).
   */
  readonly stretches: readonly (readonly number[])[];
}

/**
 * The distance of a recorded track, the way the server computes it (docs/spec/mobile.md 5.4).
 * The server's figure is the one that counts; the device uses this one for the preview, and a
 * file of test vectors that both sides run keeps the two from drifting apart
 * (`track.fixtures.json`, read by `crates/modules/mileage` as well).
 *
 * Fixes are taken in order of `seq`. A fix recorded more than `EARLY_FIX_MS` before
 * `startedAt` is dropped, and so is a fix whose accuracy radius is above `maxAccuracyM`. Each
 * further fix is compared with the last accepted one: if the step would need a speed above
 * `maxSpeedMps` (or goes back in time by any distance) it is held back and the next fix is
 * compared with the same last accepted fix again, so a real move after a gap in the signal is
 * accepted once enough time has passed. Fixes held back in a row that agree with each other (each
 * step within the cap) form a run; a fix that agrees with neither starts a new run. When a run
 * reaches `REANCHOR_RUN` fixes the track re-anchors: the run is accepted, its own steps count, and
 * the jump to it does not. A held-back fix that never joins such a run is skipped for its speed.
 * A step of zero metres is accepted at any speed.
 */
export function analyseTrack(fixes: readonly Fix[], options: TrackOptions = {}): TrackAnalysis {
  const maxSpeed = options.maxSpeedMps ?? MAX_SPEED_MPS;
  const maxAccuracy = options.maxAccuracyM ?? MAX_ACCURACY_M;
  const notBefore =
    options.startedAt == null ? Number.NEGATIVE_INFINITY : options.startedAt - EARLY_FIX_MS;
  const sorted = [...fixes].sort((a, b) => a.seq - b.seq);
  let seqGaps = 0;
  for (let i = 1; i < sorted.length; i++) {
    const gap = sorted[i]!.seq - sorted[i - 1]!.seq - 1;
    if (gap > 0) seqGaps += gap;
  }
  const plausible = (a: Fix, b: Fix, d: number): boolean => {
    const dt = (b.t - a.t) / 1000;
    return dt > 0 ? d / dt <= maxSpeed : d === 0;
  };
  let distanceM = 0;
  let accepted = 0;
  let skippedAccuracy = 0;
  let skippedSpeed = 0;
  let skippedEarly = 0;
  let reanchors = 0;
  let first: Fix | null = null;
  let last: Fix | null = null;
  let stretches: number[][] = [];
  // The run of held-back fixes and the sum of its own steps.
  let run: Fix[] = [];
  let runM = 0;
  for (const p of sorted) {
    if (p.t < notBefore) {
      skippedEarly++;
      continue;
    }
    if (p.accuracyM != null && p.accuracyM > maxAccuracy) {
      skippedAccuracy++;
      continue;
    }
    if (last !== null) {
      const d = distanceMeters(last, p);
      if (!plausible(last, p, d)) {
        const tail = run.length > 0 ? run[run.length - 1]! : null;
        const step = tail === null ? 0 : distanceMeters(tail, p);
        if (tail !== null && plausible(tail, p, step)) {
          run.push(p);
          runM += step;
        } else {
          skippedSpeed += run.length;
          run = [p];
          runM = 0;
        }
        if (run.length >= REANCHOR_RUN) {
          // Nothing was driven before the jump (a phone that stood at a far-off place, the
          // emulator's default one): the trip starts where the run does.
          if (distanceM === 0) {
            first = run[0]!;
            stretches = [];
          }
          stretches.push(run.map((f) => f.seq));
          reanchors++;
          accepted += run.length;
          distanceM += runM;
          last = run[run.length - 1]!;
          run = [];
          runM = 0;
        }
        continue;
      }
      skippedSpeed += run.length;
      run = [];
      runM = 0;
      distanceM += d;
      stretches[stretches.length - 1]!.push(p.seq);
    } else {
      first = p;
      stretches.push([p.seq]);
    }
    accepted++;
    last = p;
  }
  skippedSpeed += run.length;
  return {
    distanceM,
    accepted,
    skippedAccuracy,
    skippedSpeed,
    skippedEarly,
    reanchors,
    seqGaps,
    first,
    last,
    stretches,
  };
}

/** The distance as the server stores it: metres with two decimals, as text. */
export function distanceText(distanceM: number): string {
  return distanceM.toFixed(2);
}

/**
 * The integrity flags the server stores for a trip, in a fixed order, separated by spaces:
 * `seq_gap:N speed_jump:N low_accuracy:N before_start:N reanchor:N`. A count of zero leaves its flag out; a clean track has
 * the empty string.
 */
export function integrityFlags(a: TrackAnalysis): string {
  const flags: string[] = [];
  if (a.seqGaps > 0) flags.push(`seq_gap:${a.seqGaps}`);
  if (a.skippedSpeed > 0) flags.push(`speed_jump:${a.skippedSpeed}`);
  if (a.skippedAccuracy > 0) flags.push(`low_accuracy:${a.skippedAccuracy}`);
  if (a.skippedEarly > 0) flags.push(`before_start:${a.skippedEarly}`);
  if (a.reanchors > 0) flags.push(`reanchor:${a.reanchors}`);
  return flags.join(" ");
}

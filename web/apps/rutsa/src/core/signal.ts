import { MAX_ACCURACY_M } from "@obop/trip-core";

/**
 * Why the phone's own distance can stay at 0 km while a trip records. `analyseTrack` drops a
 * position whose accuracy radius is over the limit, and one that would need a speed over the limit
 * from the last position that counted; the distance is the sum of the steps between the positions
 * that counted, so fewer than 2 of them is 0 km and the driver cannot tell why. This says it, from
 * the counters of the same analysis and from the newest position; it applies no rule of its own.
 *
 * From `signalNote` of `mileage-app` (`src/core/signal.ts`); here the counters are the sums over
 * the segments of the trip (`progress.ts`).
 */

/** Fewer positions than this are the "waiting for the first position" case, which has its own text. */
const MIN_STORED = 3;

/**
 * The stored positions must span at least this long (newest stored time minus the oldest) before
 * the note is shown: a phone that has just started needs some seconds, and its first fixes are
 * often imprecise.
 */
export const MIN_SPAN_MS = 30_000;

/** A distance needs this many positions that count. */
const MIN_COUNTED = 2;

/**
 * What the driver is told. `dropped` is every position the rule left out, of `stored`. The kind
 * follows the newest stored position, not the counters, which only grow: a phone that delivers
 * good positions now is not told to move to a window.
 */
export type SignalNote =
  /** The newest position is itself over the accuracy limit; `accuracyM` in whole metres, `null` when not finite. */
  | { kind: "weak"; dropped: number; stored: number; accuracyM: number | null }
  /** The newest position is within the limit and at least one was dropped for its speed. */
  | { kind: "jumpy"; dropped: number; stored: number };

function overLimit(accuracyM: number | null | undefined): accuracyM is number {
  return accuracyM !== null && accuracyM !== undefined && accuracyM > MAX_ACCURACY_M;
}

/**
 * The note for a trip, or `null` when there is nothing to say: only with at least 3 positions
 * stored over at least `MIN_SPAN_MS`, fewer than 2 that count, and at least one dropped for its
 * accuracy or its speed. Each test is written so that a count that is not a number says nothing.
 */
export function signalNote(input: {
  accepted: number;
  skippedAccuracy: number;
  skippedSpeed: number;
  stored: number;
  spanMs: number;
  newestAccuracyM: number | null | undefined;
}): SignalNote | null {
  const { accepted, skippedAccuracy, skippedSpeed, stored, newestAccuracyM } = input;
  if (!(stored >= MIN_STORED)) return null;
  if (!(input.spanMs >= MIN_SPAN_MS)) return null;
  if (!(accepted < MIN_COUNTED)) return null;
  if (!(skippedAccuracy > 0 || skippedSpeed > 0)) return null;
  const dropped = skippedAccuracy + skippedSpeed;
  if (overLimit(newestAccuracyM)) {
    const accuracyM = Number.isFinite(newestAccuracyM) ? Math.round(newestAccuracyM) : null;
    return { kind: "weak", dropped, stored, accuracyM };
  }
  if (skippedSpeed > 0) return { kind: "jumpy", dropped, stored };
  return null;
}

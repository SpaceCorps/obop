import { MAX_ACCURACY_M, type TrackAnalysis } from "@obop/trip-core";

/**
 * Why the phone's own distance can stay at 0 m. `analyseTrack` (docs/spec/mobile.md 5.4) drops a
 * position whose accuracy radius is over the limit, and a position that would need a speed over
 * the limit from the last position that counted; the distance is the sum of the steps between the
 * positions that counted, so fewer than 2 of them is 0 m, and the server refuses to end such a
 * trip. The Recording screen shows "7 points, 0 m" and a driver cannot tell why. This says it,
 * from the counters of the same analysis; it applies no rule of its own and changes none.
 */

/** Fewer positions than this are the "waiting for the first position" case, which has its own text. */
const MIN_STORED = 3;

/**
 * The stored positions must span at least this long (the newest stored time minus the oldest, in
 * milliseconds) before the note is shown. A phone that has just started needs some seconds to find
 * its position, and its first fixes are often imprecise; three of them, a few seconds apart, are
 * not a signal that is too weak.
 */
export const MIN_SPAN_MS = 30_000;

/** A distance needs this many positions that count. */
const MIN_COUNTED = 2;

/**
 * What the driver is told. `dropped` is every position the rule left out, for either reason, of
 * `stored` positions. The kind is the reason that was the more common one.
 */
export type SignalNote =
  /** At least as many were dropped for their accuracy as for their speed. `accuracyM` is the newest position's radius in whole metres, `null` when it is not known or not over the limit. */
  | { kind: "weak"; dropped: number; stored: number; accuracyM: number | null }
  /** More were dropped for their speed (they do not fit the first position that counted) than for their accuracy. */
  | { kind: "jumpy"; dropped: number; stored: number };

/**
 * The radius of the newest position as the line shows it: whole metres, and only when that
 * position is itself over the limit the rule drops at. A radius of 20 m beside "too imprecise"
 * would contradict the sentence; a missing, zero, negative or non-numeric radius is not known.
 */
function shownAccuracy(accuracyM: number | null | undefined): number | null {
  if (accuracyM === null || accuracyM === undefined) return null;
  if (!Number.isFinite(accuracyM) || !(accuracyM > MAX_ACCURACY_M)) return null;
  return Math.round(accuracyM);
}

/**
 * The note for a trip, or `null` when there is nothing new to say. It appears only when at least 3
 * positions are stored and they span at least `MIN_SPAN_MS`, fewer than 2 of them count and at
 * least one was dropped for its accuracy or its speed: with 2 or more that count the distance is a
 * distance, with fewer than 3 stored (or all of them in the first seconds) the phone is still
 * finding its position, and with nothing dropped it simply delivered few positions. It is "weak"
 * when at least as many were dropped for their accuracy as for their speed, otherwise "jumpy".
 *
 * `analysis` is the result of `analyseTrack` over the stored positions (the counters are used as
 * they are), `stored` the number of stored positions, `spanMs` the newest stored time minus the
 * oldest one, `newestAccuracyM` the accuracy radius in metres of the newest stored position, `null`
 * or `undefined` when it has none.
 */
export function signalNote(input: {
  analysis: Pick<TrackAnalysis, "accepted" | "skippedAccuracy" | "skippedSpeed">;
  stored: number;
  spanMs: number;
  newestAccuracyM: number | null | undefined;
}): SignalNote | null {
  const { analysis, stored } = input;
  const { accepted, skippedAccuracy, skippedSpeed } = analysis;
  // Each test is written so that a count that is not a number says nothing.
  if (!(stored >= MIN_STORED)) return null;
  if (!(input.spanMs >= MIN_SPAN_MS)) return null;
  if (!(accepted < MIN_COUNTED)) return null;
  if (!(skippedAccuracy > 0 || skippedSpeed > 0)) return null;
  const dropped = skippedAccuracy + skippedSpeed;
  if (skippedAccuracy >= skippedSpeed) {
    return { kind: "weak", dropped, stored, accuracyM: shownAccuracy(input.newestAccuracyM) };
  }
  return { kind: "jumpy", dropped, stored };
}

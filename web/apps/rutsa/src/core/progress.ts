import { analyseTrack, type Fix } from "@obop/trip-core";
import { lastFixOfSegment } from "./liveness.ts";
import { signalNote, type SignalNote } from "./signal.ts";
import type { StoredPoint, Trip } from "./types.ts";

/** The analysis of a recorded trip that the distance and the note about 0 km both come from. */
export interface RecordedAnalysis {
  /** Metres, not rounded. */
  distanceM: number;
  /** The positions that count, over all segments. */
  accepted: number;
  /** Dropped for their accuracy radius, over all segments. */
  skippedAccuracy: number;
  /** Dropped for the speed they would need, over all segments. */
  skippedSpeed: number;
}

/**
 * `analyseTrack` of `@obop/trip-core` over each segment on its own, summed, so a pause adds no
 * straight line from where the trip paused to where it went on. Each segment is analysed from its
 * own start (`starts`, by segment number; the start of the trip where a segment has none), the
 * same rule `acceptFixes` applied when its positions were stored. Stop takes its distance from
 * here, and the count while a trip records too, so the two are the same.
 */
export function analyseRecorded(
  points: readonly StoredPoint[],
  starts: ReadonlyMap<number, number>,
  startedAt: number,
): RecordedAnalysis {
  const segments = new Map<number, Fix[]>();
  for (const p of points) {
    const fixes = segments.get(p.segment) ?? [];
    fixes.push({ seq: p.seq, t: p.t, lat: p.lat, lon: p.lon, accuracyM: p.accuracyM });
    segments.set(p.segment, fixes);
  }
  const sum: RecordedAnalysis = { distanceM: 0, accepted: 0, skippedAccuracy: 0, skippedSpeed: 0 };
  for (const [segment, fixes] of segments) {
    const a = analyseTrack(fixes, { startedAt: starts.get(segment) ?? startedAt });
    sum.distanceM += a.distanceM;
    sum.accepted += a.accepted;
    sum.skippedAccuracy += a.skippedAccuracy;
    sum.skippedSpeed += a.skippedSpeed;
  }
  return sum;
}

/** When a segment started and ended (Pause or Stop); `endedAt` is `null` while it is open. */
export interface SegmentTimes {
  segment: number;
  startedAt: number;
  endedAt: number | null;
}

/** The count of a trip while it records, for the home screen. */
export interface TripProgress {
  /** The state of the trip when this was read: a count read before a Pause is not one after it. */
  state: Trip["state"];
  /**
   * The time of the segments that have ended, in milliseconds. The time of the trip is this plus,
   * while it records, the time since the start of its open segment (`elapsedMs` of
   * `ui/recording-view.ts`): the clock stands while the trip is paused.
   */
  closedMs: number;
  /** The km so far: the same count Stop makes. */
  distanceM: number;
  /** Stored positions of the whole trip. */
  points: number;
  /**
   * Of those, the ones the distance leaves out for their accuracy or the speed they would need
   * (the same rule as Stop); the screen of a trip (D6) says it next to `points`.
   */
  dropped: number;
  /** When the newest position of the current segment was taken; `null` when it has none. */
  lastFixAt: number | null;
  /** Why the distance may stay at 0 km (`signal.ts`); `null` when there is nothing to say. */
  signal: SignalNote | null;
}

/**
 * The count of an open trip from its stored positions and the starts of its segments. The span the
 * note needs is the newest stored time minus the oldest; the newest position is the one stored
 * last (the highest `seq`).
 *
 * After `progressFrom` of `mileage-app` (`src/core/progress.ts`), over the segments.
 */
export function progressOf(
  trip: Pick<Trip, "startedAt" | "segment" | "state">,
  points: readonly StoredPoint[],
  segments: readonly SegmentTimes[],
): TripProgress {
  const starts = new Map(segments.map((s) => [s.segment, s.startedAt]));
  const analysis = analyseRecorded(points, starts, trip.startedAt);
  let closedMs = 0;
  for (const s of segments) {
    if (s.endedAt !== null) closedMs += Math.max(0, s.endedAt - s.startedAt);
  }
  let first: number | null = null;
  let last: number | null = null;
  let newest: StoredPoint | null = null;
  for (const p of points) {
    first = first === null ? p.t : Math.min(first, p.t);
    last = last === null ? p.t : Math.max(last, p.t);
    if (newest === null || p.seq > newest.seq) newest = p;
  }
  return {
    state: trip.state,
    closedMs,
    distanceM: analysis.distanceM,
    points: points.length,
    dropped: analysis.skippedAccuracy + analysis.skippedSpeed,
    lastFixAt: lastFixOfSegment(points, trip.segment),
    signal: signalNote({
      ...analysis,
      stored: points.length,
      spanMs: first === null || last === null ? 0 : last - first,
      newestAccuracyM: newest?.accuracyM ?? null,
    }),
  };
}

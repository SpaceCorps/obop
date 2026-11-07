import { analyseTrack, type Fix } from "@obop/trip-core";
import { signalNote, type SignalNote } from "./signal.ts";
import type { StoredPoint } from "./types.ts";

/** The phone's own count of the trip so far, for the Recording screen. */
export interface Progress {
  /** Stored points, including the ones the distance ignores. */
  points: number;
  /** The distance of the points that count, in metres. */
  metres: number;
  /** When the newest stored position was taken (ms), or `null`. */
  lastFixMs: number | null;
  /** Why the distance may stay at 0 m (`signal.ts`), or `null` when there is nothing to say. */
  signal: SignalNote | null;
}

/**
 * The count of a trip from its stored points: the same rules as the server (`trip-core`), for the
 * preview only. The track is analysed once; the distance and the note both come from that analysis.
 * The newest position is the one with the highest `seq` (the order the points were stored in); the
 * span the note needs (`MIN_SPAN_MS`) is the newest time stamp minus the oldest one of the stored
 * positions, whatever their order.
 */
export function progressFrom(stored: readonly StoredPoint[]): Progress {
  const fixes: Fix[] = stored.map((p) => ({
    seq: p.seq,
    t: p.t,
    lat: Number(p.lat),
    lon: Number(p.lon),
    accuracyM: p.accuracyM === null ? null : Number(p.accuracyM),
  }));
  let firstFixMs: number | null = null;
  let lastFixMs: number | null = null;
  let newest: Fix | null = null;
  for (const fix of fixes) {
    firstFixMs = firstFixMs === null ? fix.t : Math.min(firstFixMs, fix.t);
    lastFixMs = lastFixMs === null ? fix.t : Math.max(lastFixMs, fix.t);
    if (newest === null || fix.seq > newest.seq) newest = fix;
  }
  const analysis = analyseTrack(fixes);
  return {
    points: stored.length,
    metres: analysis.distanceM,
    lastFixMs,
    signal: signalNote({
      analysis,
      stored: stored.length,
      spanMs: firstFixMs === null || lastFixMs === null ? 0 : lastFixMs - firstFixMs,
      newestAccuracyM: newest === null ? null : newest.accuracyM,
    }),
  };
}

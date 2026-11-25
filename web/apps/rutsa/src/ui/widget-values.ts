import { dayKey, formatClock, formatKm } from "../core/present.ts";
import type { Trip } from "../core/types.ts";
import { tripDurationMs, whenText } from "./home.ts";
import { elapsedMs, formatElapsed } from "./recording-view.ts";

/**
 * What the current widget shows of a trip, the same on the home screen and on the trip's screen:
 * its time without the pauses (`H:MM:SS`; running while it records, standing while it is paused,
 * the whole time once it ended), its km (so far while it is open, by the count; as stored once it
 * ended) and the time at its bottom right: when it started while it is open, when it ended once
 * it ended (the clock time today, else with its day). Pure, so it is tested.
 */
export function widgetValues(
  trip: Pick<Trip, "state" | "source" | "startedAt" | "endedAt" | "distanceM" | "segmentStartedAt">,
  /** The count of the trip (`progress` of the store): `null` while it is read, and for a manual trip. */
  count: { state: Trip["state"]; closedMs: number; distanceM: number } | null,
  now: number,
  locale: string,
  timeZone: string,
  words: { today: string; yesterday: string },
): { elapsed: string; km: string; clock: string } {
  const open = trip.state !== "ended";
  const ms = open
    ? count === null
      ? 0
      : elapsedMs({
          // The state the count was read in: right after a Pause or a Resume, until it is read
          // again (at once), the clock goes on as it went, and nothing jumps.
          state: count.state,
          closedMs: count.closedMs,
          segmentStartedAt: trip.segmentStartedAt,
          now,
        })
    : (tripDurationMs(trip, count?.closedMs ?? null) ?? 0);
  const metres = open ? (count?.distanceM ?? 0) : (trip.distanceM ?? 0);
  const at = open ? trip.startedAt : (trip.endedAt ?? trip.startedAt);
  const clock =
    dayKey(at, timeZone) === dayKey(now, timeZone)
      ? formatClock(at, locale, timeZone)
      : whenText(at, now, locale, timeZone, words);
  return { elapsed: formatElapsed(ms), km: formatKm(metres, locale), clock };
}

import { dayBefore, dayKey, formatClock, formatShortDate } from "../core/present.ts";
import type { Trip } from "../core/types.ts";

/**
 * What the home screen shows besides the totals (`totals` of `core/present.ts`): the last trip and
 * when it was. Pure, so it is tested.
 */

/** The newest trip that has ended, recorded or added by hand; `null` before the first. */
export function lastEndedTrip(trips: readonly Trip[]): Trip | null {
  let last: Trip | null = null;
  for (const trip of trips) {
    if (trip.state === "ended" && (last === null || trip.startedAt > last.startedAt)) last = trip;
  }
  return last;
}

/**
 * When a moment was, for a line on the screen: `Today 14:05`, `Yesterday 08:30`, or the day
 * without its year (`Nov 18 08:30`), in the time zone given. The words come from the catalogue.
 */
export function whenText(
  ms: number,
  now: number,
  locale: string,
  timeZone: string,
  words: { today: string; yesterday: string },
): string {
  const day = dayKey(ms, timeZone);
  const today = dayKey(now, timeZone);
  const clock = formatClock(ms, locale, timeZone);
  if (day === today) return `${words.today} ${clock}`;
  if (day === dayBefore(today)) return `${words.yesterday} ${clock}`;
  return `${formatShortDate(ms, locale, timeZone)} ${clock}`;
}

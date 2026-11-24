import { dayKey, formatClock, formatDay } from "../core/present.ts";
import type { Trip } from "../core/types.ts";

/**
 * The date and the times of a trip on its screen (D6), in the time zone given. Pure, so it is
 * tested.
 */

/** The day of its start with its weekday: `Mon, Nov 23`, with the year when it is not this year. */
export function tripDate(
  trip: Pick<Trip, "startedAt">,
  now: number,
  locale: string,
  timeZone: string,
): string {
  const year = dayKey(trip.startedAt, timeZone).slice(0, 4);
  return formatDay(trip.startedAt, locale, timeZone, year !== dayKey(now, timeZone).slice(0, 4));
}

/**
 * `08:10–08:42`. A trip that ends on another day than it started says that day after the end:
 * `23:40–00:20 (Sat, Nov 21)`, with the year when the trip goes into a new year.
 */
export function tripTimes(
  trip: Pick<Trip, "startedAt" | "endedAt">,
  locale: string,
  timeZone: string,
): string {
  const start = formatClock(trip.startedAt, locale, timeZone);
  if (trip.endedAt === null) return start;
  const end = formatClock(trip.endedAt, locale, timeZone);
  const startDay = dayKey(trip.startedAt, timeZone);
  const endDay = dayKey(trip.endedAt, timeZone);
  if (startDay === endDay) return `${start}–${end}`;
  const withYear = startDay.slice(0, 4) !== endDay.slice(0, 4);
  return `${start}–${end} (${formatDay(trip.endedAt, locale, timeZone, withYear)})`;
}

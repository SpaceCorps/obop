import type { Trip } from "./types.ts";

/**
 * Trips as the driver sees them: kilometres, clock times, durations, trips by day and the totals
 * of today and this month. Pure functions without words: numbers and times are written by `Intl`
 * for the language given (`"en"`, later `"sv"`), and the words around them ("Today", "min") are
 * the screens' (`src/i18n`).
 *
 * Times are stored in UTC; days and clock times follow the time zone given, which on the phone is
 * the phone's own (`Intl.DateTimeFormat().resolvedOptions().timeZone`). A trip belongs to the day
 * and the month of its start, also when it ends after midnight.
 */

const dayFormats = new Map<string, Intl.DateTimeFormat>();

function dayFormat(timeZone: string): Intl.DateTimeFormat {
  let format = dayFormats.get(timeZone);
  if (!format) {
    format = new Intl.DateTimeFormat("en-GB", {
      timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    });
    dayFormats.set(timeZone, format);
  }
  return format;
}

/** The day of a moment in a time zone, as `YYYY-MM-DD`; such keys sort as the days do. */
export function dayKey(ms: number, timeZone: string): string {
  const parts = dayFormat(timeZone).formatToParts(new Date(ms));
  const part = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((p) => p.type === type)?.value ?? "";
  return `${part("year")}-${part("month")}-${part("day")}`;
}

/** The month of a moment in a time zone, as `YYYY-MM`. */
export function monthKey(ms: number, timeZone: string): string {
  return dayKey(ms, timeZone).slice(0, 7);
}

/** The distance of a trip for the sums: a trip still recording has none yet and counts 0. */
const metres = (trip: Trip): number => trip.distanceM ?? 0;

export interface DayOfTrips {
  /** `YYYY-MM-DD` in the time zone given. */
  day: string;
  /** Newest first. */
  trips: Trip[];
  distanceM: number;
}

/** The trips grouped by the day of their start, newest day first. */
export function byDay(trips: readonly Trip[], timeZone: string): DayOfTrips[] {
  const days = new Map<string, Trip[]>();
  for (const trip of trips) {
    const day = dayKey(trip.startedAt, timeZone);
    const list = days.get(day) ?? [];
    list.push(trip);
    days.set(day, list);
  }
  return [...days.entries()]
    .sort(([a], [b]) => (a < b ? 1 : a > b ? -1 : 0))
    .map(([day, list]) => ({
      day,
      trips: [...list].sort((a, b) => b.startedAt - a.startedAt),
      distanceM: list.reduce((sum, trip) => sum + metres(trip), 0),
    }));
}

/** The metres of the trips that started today and this month, in the time zone given. */
export function totals(
  trips: readonly Trip[],
  now: number,
  timeZone: string,
): { todayM: number; monthM: number } {
  const today = dayKey(now, timeZone);
  const month = today.slice(0, 7);
  let todayM = 0;
  let monthM = 0;
  for (const trip of trips) {
    const day = dayKey(trip.startedAt, timeZone);
    if (day === today) todayM += metres(trip);
    if (day.slice(0, 7) === month) monthM += metres(trip);
  }
  return { todayM, monthM };
}

/** Metres as kilometres with one decimal: `12.4 km` in English, `12,4 km` in Swedish. */
export function formatKm(distanceM: number, locale: string): string {
  const km = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(distanceM / 1000);
  return `${km} km`;
}

/** The clock time of a moment in a time zone, 24 hours: `09:05`. */
export function formatClock(ms: number, locale: string, timeZone: string): string {
  return new Intl.DateTimeFormat(locale, {
    timeZone,
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(new Date(ms));
}

/**
 * How long a trip took, rounded to the minute, in hours and minutes; the screen puts the words
 * around them. A trip still recording passes the current time as its end.
 */
export function durationParts(
  startedAt: number,
  endedAt: number,
): { hours: number; minutes: number } {
  const total = Math.max(0, Math.round((endedAt - startedAt) / 60_000));
  return { hours: Math.floor(total / 60), minutes: total % 60 };
}

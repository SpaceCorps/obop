import {
  byDay,
  dayBefore,
  dayKey,
  formatClock,
  formatDay,
  formatKm,
  formatShortDate,
} from "../core/present.ts";
import type { Trip } from "../core/types.ts";
import { tripTitle } from "./trip-label.ts";

/**
 * What the History shows (D5): the trips that ended, by the day of their start, newest first, each
 * day with its name, its number of trips and its km, and each trip as one line. The open trip is
 * not history yet: the home screen shows it. Pure, so it is tested; the words come from the
 * catalogue through `words`.
 */

export interface ListWords {
  today: string;
  /** "3 trips", in the plural form of the count. */
  trips: (count: number) => string;
  /** The mark of a trip added by hand (BR-28). */
  manual: string;
}

export interface TripLine {
  id: string;
  /** Its name, else From → To, else its times. */
  title: string;
  /** Its times when the title is a name or places, and the mark of a trip added by hand. */
  subtitle: string | null;
  km: string;
}

export interface Section {
  /** The day as `YYYY-MM-DD` in the time zone given. */
  key: string;
  /** `Today · 2 trips · 12.4 km`, `Wed, Nov 18 · 1 trip · 3.0 km`. */
  header: string;
  trips: TripLine[];
}

/** The name of a day: Today, else its weekday and date, with the year if not this year's. */
function dayName(
  day: string,
  ms: number,
  now: number,
  locale: string,
  timeZone: string,
  words: ListWords,
): string {
  const today = dayKey(now, timeZone);
  if (day === today) return words.today;
  return formatDay(ms, locale, timeZone, day.slice(0, 4) !== today.slice(0, 4));
}

/** `08:30–09:15`. A trip over midnight ends on the next day's clock. */
function timesOf(trip: Trip, locale: string, timeZone: string): string {
  const start = formatClock(trip.startedAt, locale, timeZone);
  if (trip.endedAt === null) return start;
  return `${start}–${formatClock(trip.endedAt, locale, timeZone)}`;
}

/** One trip that ended as a line of the list. */
export function tripLine(trip: Trip, locale: string, timeZone: string, words: ListWords): TripLine {
  const times = timesOf(trip, locale, timeZone);
  const title = tripTitle(trip);
  const subtitle = [title === null ? null : times, trip.source === "manual" ? words.manual : null]
    .filter((part) => part !== null)
    .join(" · ");
  return {
    id: trip.id,
    title: title ?? times,
    subtitle: subtitle === "" ? null : subtitle,
    km: formatKm(trip.distanceM ?? 0, locale),
  };
}

/**
 * A line of the recent trips of the home screen: the line of History, with the day in front of
 * the times, since the list has no headers of days. `Meeting` over `Today 08:30–09:15`; an untitled
 * trip `08:30–09:15` over `Yesterday`; a day before yesterday as `Nov 18`.
 */
export function recentLine(
  trip: Trip,
  now: number,
  locale: string,
  timeZone: string,
  words: ListWords & { yesterday: string },
): TripLine {
  const times = timesOf(trip, locale, timeZone);
  const title = tripTitle(trip);
  const day = dayKey(trip.startedAt, timeZone);
  const today = dayKey(now, timeZone);
  const dayText =
    day === today
      ? words.today
      : day === dayBefore(today)
        ? words.yesterday
        : formatShortDate(trip.startedAt, locale, timeZone);
  const subtitle = [
    title === null ? dayText : `${dayText} ${times}`,
    trip.source === "manual" ? words.manual : null,
  ]
    .filter((part) => part !== null)
    .join(" · ");
  return {
    id: trip.id,
    title: title ?? times,
    subtitle,
    km: formatKm(trip.distanceM ?? 0, locale),
  };
}

/** The trips that ended, newest first: the recent trips of the home screen take the first ones. */
export function endedNewestFirst(trips: readonly Trip[]): Trip[] {
  return trips.filter((trip) => trip.state === "ended").sort((a, b) => b.startedAt - a.startedAt);
}

/**
 * The list: the days of the trips that ended, newest first, the trips of a day newest first. Empty
 * when no trip has ended. The open trip is left out, so the km of today are those of Today on the
 * home screen.
 */
export function tripSections(
  trips: readonly Trip[],
  now: number,
  locale: string,
  timeZone: string,
  words: ListWords,
): Section[] {
  const ended = trips.filter((trip) => trip.state === "ended");
  return byDay(ended, timeZone).map(({ day, trips: list, distanceM }) => ({
    key: day,
    header: [
      dayName(day, list[0]?.startedAt ?? now, now, locale, timeZone, words),
      words.trips(list.length),
      formatKm(distanceM, locale),
    ].join(" · "),
    trips: list.map((trip) => tripLine(trip, locale, timeZone, words)),
  }));
}

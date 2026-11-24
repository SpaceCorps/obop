import { byDay, dayKey, formatClock, formatDay, formatKm } from "../core/present.ts";
import type { Trip } from "../core/types.ts";
import { tripTitle } from "./trip-label.ts";

/**
 * What the list of trips shows (D5): the open trip on top under Current, then the trips that ended
 * by the day of their start, newest first, each day with its name, its number of trips and its km,
 * and each trip as one line. Pure, so it is tested; the words come from the catalogue through
 * `words`.
 */

export interface ListWords {
  /** The header of the open trip. */
  current: string;
  today: string;
  /** "3 trips", in the plural form of the count. */
  trips: (count: number) => string;
  /** The mark of a trip added by hand (BR-28). */
  manual: string;
  recording: string;
  paused: string;
}

export interface TripLine {
  id: string;
  /** Its name, else From → To, else its times. */
  title: string;
  /** Its times when the title is a name or places, the mark of a trip added by hand, the state of an open trip. */
  subtitle: string | null;
  /** `null` for the open trip: Current on the home screen counts it. */
  km: string | null;
  /** The trip records or is paused: it is shown on the home screen, not on a screen of its own. */
  open: boolean;
}

export interface Section {
  /** `current` for the open trip, else the day as `YYYY-MM-DD` in the time zone given. */
  key: string;
  /** `Current`, or `Today · 2 trips · 12.4 km`, `Wed, Nov 18 · 1 trip · 3.0 km`. */
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

/** `08:30–09:15`; an open trip has only its start. A trip over midnight ends on the next day's clock. */
function timesOf(trip: Trip, locale: string, timeZone: string): string {
  const start = formatClock(trip.startedAt, locale, timeZone);
  if (trip.endedAt === null) return start;
  return `${start}–${formatClock(trip.endedAt, locale, timeZone)}`;
}

/** One trip as a line of the list. */
export function tripLine(trip: Trip, locale: string, timeZone: string, words: ListWords): TripLine {
  const times = timesOf(trip, locale, timeZone);
  const title = tripTitle(trip);
  const open = trip.state !== "ended";
  const subtitle = [
    title === null ? null : times,
    trip.source === "manual" ? words.manual : null,
    trip.state === "recording" ? words.recording : trip.state === "paused" ? words.paused : null,
  ]
    .filter((part) => part !== null)
    .join(" · ");
  return {
    id: trip.id,
    title: title ?? times,
    subtitle: subtitle === "" ? null : subtitle,
    km: open ? null : formatKm(trip.distanceM ?? 0, locale),
    open,
  };
}

/**
 * The list: Current with the open trip when there is one (there is one at most), then the days of
 * the trips that ended, newest first, the trips of a day newest first. Empty when there are no
 * trips. The days leave the open trip out, so their km are those of Today on the home screen.
 */
export function tripSections(
  trips: readonly Trip[],
  now: number,
  locale: string,
  timeZone: string,
  words: ListWords,
): Section[] {
  const open = trips.filter((trip) => trip.state !== "ended");
  const ended = trips.filter((trip) => trip.state === "ended");
  const current: Section[] =
    open.length === 0
      ? []
      : [
          {
            key: "current",
            header: words.current,
            trips: open.map((trip) => tripLine(trip, locale, timeZone, words)),
          },
        ];
  const days = byDay(ended, timeZone).map(({ day, trips: list, distanceM }) => ({
    key: day,
    header: [
      dayName(day, list[0]?.startedAt ?? now, now, locale, timeZone, words),
      words.trips(list.length),
      formatKm(distanceM, locale),
    ].join(" · "),
    trips: list.map((trip) => tripLine(trip, locale, timeZone, words)),
  }));
  return [...current, ...days];
}

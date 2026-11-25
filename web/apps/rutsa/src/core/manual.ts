import { DETAIL_MAX, cleanText, codePoints } from "./details.ts";
import { dayAfter, dayKey, momentOf } from "./present.ts";
import type { ManualTrip } from "./repository.ts";
import type { Trip } from "./types.ts";

/**
 * The checks of a trip added by hand (D7, BR-28), on the texts of the form as typed. Pure, so they
 * are tested; the screen shows each problem under its field and the words come from the
 * catalogue. Times are typed in the phone's time zone and stored in UTC, like recorded ones.
 */

/** The most km a typed trip may have: more is a slip of the finger. */
export const MAX_KM = 2000;
/** Above this average speed the driver is asked whether the trip is right (not refused). */
export const FAST_KMH = 150;

/** The form as typed. */
export interface ManualForm {
  name: string;
  /** `YYYY-MM-DD`. */
  date: string;
  /** `08:30` (also `8:30`, `8.30`, `0830` or `830`). */
  start: string;
  end: string;
  /** `12.4` or `12,4`. */
  km: string;
  from: string;
  to: string;
  note: string;
}

export type ManualField = keyof ManualForm;

export type ManualProblem =
  | "date_format"
  | "date_future"
  | "time_format"
  | "time_future"
  | "time_same"
  | "overlap"
  | "km_format"
  | "km_zero"
  | "km_too_far"
  | "required"
  | "too_long";

export interface ManualCheck {
  /** At most one problem per field. */
  problems: Partial<Record<ManualField, ManualProblem>>;
  /** The end is before the start, so it is on the next day (said under the field). */
  nextDay: boolean;
  /** The average speed, once the times and the km are right; asked about over `FAST_KMH`. */
  speedKmh: number | null;
  /** What is stored, when there is no problem. */
  trip: ManualTrip | null;
}

/** A real day `YYYY-MM-DD`, else `null`. */
function parseDay(text: string): string | null {
  const day = text.trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return null;
  const [year, month, date] = day.split("-").map(Number) as [number, number, number];
  const back = new Date(Date.UTC(year, month - 1, date)).toISOString().slice(0, 10);
  return back === day ? day : null;
}

/**
 * `08:30`, `8:30`, `8.30`, `0830` or `830` as hours and minutes, else `null`: the number pad of
 * Android has no colon.
 */
function parseClock(text: string): { hour: number; minute: number } | null {
  const match = /^(\d{1,2})[:.]?(\d{2})$/.exec(text.trim());
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  return hour < 24 && minute < 60 ? { hour, minute } : null;
}

/** `12.4` or `12,4` as a number, else `null`. */
function parseKm(text: string): number | null {
  const km = text.trim().replace(",", ".");
  return /^\d+(\.\d+)?$/.test(km) ? Number(km) : null;
}

/** A required place: given and not too long. */
function placeProblem(text: string, field: "from" | "to"): ManualProblem | null {
  const value = cleanText(text);
  if (value === null) return "required";
  return codePoints(value) > DETAIL_MAX[field] ? "too_long" : null;
}

/** An optional text: not too long. */
function optionalProblem(text: string, field: "name" | "note"): ManualProblem | null {
  const value = cleanText(text);
  return value !== null && codePoints(value) > DETAIL_MAX[field] ? "too_long" : null;
}

/**
 * The checks of the form against the clock (`now`), the phone's time zone and the trips on the
 * phone: the open one counts until now, since its km would be counted twice.
 */
export function checkManual(
  form: ManualForm,
  now: number,
  timeZone: string,
  trips: readonly Pick<Trip, "startedAt" | "endedAt">[],
): ManualCheck {
  const problems: ManualCheck["problems"] = {};
  let nextDay = false;
  let startedAt: number | null = null;
  let endedAt: number | null = null;

  const day = parseDay(form.date);
  if (day === null) problems.date = "date_format";
  else if (day > dayKey(now, timeZone)) problems.date = "date_future";

  const start = parseClock(form.start);
  const end = parseClock(form.end);
  if (start === null) problems.start = "time_format";
  if (end === null) problems.end = "time_format";

  if (day !== null && problems.date === undefined && start !== null && end !== null) {
    startedAt = momentOf(day, start.hour, start.minute, timeZone);
    const sameDayEnd = momentOf(day, end.hour, end.minute, timeZone);
    if (sameDayEnd === startedAt) {
      problems.end = "time_same";
    } else if (sameDayEnd < startedAt) {
      nextDay = true;
      endedAt = momentOf(dayAfter(day), end.hour, end.minute, timeZone);
    } else {
      endedAt = sameDayEnd;
    }
    if (startedAt > now) problems.start = "time_future";
    if (endedAt !== null && endedAt > now) problems.end = "time_future";
  }

  if (
    startedAt !== null &&
    endedAt !== null &&
    problems.start === undefined &&
    problems.end === undefined
  ) {
    const from = startedAt;
    const to = endedAt;
    const overlaps = trips.some((trip) => from < (trip.endedAt ?? now) && trip.startedAt < to);
    if (overlaps) problems.start = "overlap";
  }

  const km = parseKm(form.km);
  if (km === null) problems.km = "km_format";
  else if (km <= 0) problems.km = "km_zero";
  else if (km > MAX_KM) problems.km = "km_too_far";

  const fromProblem = placeProblem(form.from, "from");
  if (fromProblem) problems.from = fromProblem;
  const toProblem = placeProblem(form.to, "to");
  if (toProblem) problems.to = toProblem;
  const nameProblem = optionalProblem(form.name, "name");
  if (nameProblem) problems.name = nameProblem;
  const noteProblem = optionalProblem(form.note, "note");
  if (noteProblem) problems.note = noteProblem;

  const timesRight =
    startedAt !== null &&
    endedAt !== null &&
    problems.start === undefined &&
    problems.end === undefined;
  const speedKmh =
    timesRight && km !== null && problems.km === undefined && startedAt !== null && endedAt !== null
      ? km / ((endedAt - startedAt) / 3_600_000)
      : null;

  const ok = Object.keys(problems).length === 0 && startedAt !== null && endedAt !== null;
  return {
    problems,
    nextDay,
    speedKmh,
    trip:
      ok && startedAt !== null && endedAt !== null && km !== null
        ? {
            startedAt,
            endedAt,
            distanceM: Math.round(km * 1000),
            name: cleanText(form.name),
            from: cleanText(form.from),
            to: cleanText(form.to),
            note: cleanText(form.note),
          }
        : null,
  };
}

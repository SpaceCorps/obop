import { DETAIL_MAX, cleanText, codePoints } from "./details.ts";
import { dayKey, momentOf } from "./present.ts";
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
  /** How long it took, `1:15:30` (hours, minutes and seconds; `parseDuration`); the end follows. */
  duration: string;
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
  | "duration_format"
  | "duration_zero"
  | "end_future"
  | "overlap"
  | "km_format"
  | "km_zero"
  | "km_too_far"
  | "required"
  | "too_long";

export interface ManualCheck {
  /** At most one problem per field. */
  problems: Partial<Record<ManualField, ManualProblem>>;
  /** The end is on the day after the start (said under the times). */
  nextDay: boolean;
  /** The average speed, once the times and the km are right; asked about over `FAST_KMH`. */
  speedKmh: number | null;
  /**
   * The start and the end (the start and the duration) once the date, the start and the duration
   * are times, also when they are still to come or meet another trip (said under them): the
   * screen shows them as `08:30–09:15`.
   */
  times: { startedAt: number; endedAt: number } | null;
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

/**
 * `1:15:30` (hours, minutes and seconds) as seconds, up to `23:59:59`; also `1:15` (no seconds)
 * and the forms of `start` (`1.15`, `115`, `01:15`); else `null`.
 */
export function parseDuration(text: string): number | null {
  const full = /^(\d{1,2}):(\d{2}):(\d{2})$/.exec(text.trim());
  if (full) {
    const [hour, minute, second] = [Number(full[1]), Number(full[2]), Number(full[3])];
    return hour < 24 && minute < 60 && second < 60 ? hour * 3600 + minute * 60 + second : null;
  }
  const clock = parseClock(text);
  return clock === null ? null : clock.hour * 3600 + clock.minute * 60;
}

/**
 * The duration from its three fields as typed (hours, minutes, seconds): a field left empty is 0.
 * Gives the text `parseDuration` reads; a field that is not a number, or minutes or seconds over
 * 59, give a text it refuses.
 */
export function durationFromParts(hours: string, minutes: string, seconds: string): string {
  const part = (text: string) => (text.trim() === "" ? "0" : text.trim());
  return `${part(hours)}:${part(minutes).padStart(2, "0")}:${part(seconds).padStart(2, "0")}`;
}

/** `4530` seconds as `1:15:30`, the form the duration is kept in. */
export function durationText(seconds: number): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${Math.floor(seconds / 3600)}:${two(Math.floor((seconds % 3600) / 60))}:${two(seconds % 60)}`;
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
  const seconds = parseDuration(form.duration);
  if (start === null) problems.start = "time_format";
  if (seconds === null) problems.duration = "duration_format";
  else if (seconds === 0) problems.duration = "duration_zero";

  if (day !== null && problems.date === undefined && start !== null && seconds !== null) {
    startedAt = momentOf(day, start.hour, start.minute, timeZone);
    // The time it took, as it passed: a change of the clock in between does not add or take an hour.
    endedAt = startedAt + seconds * 1000;
    nextDay = dayKey(endedAt, timeZone) !== day;
    if (startedAt > now) problems.start = "time_future";
    else if (endedAt > now && problems.duration === undefined) problems.duration = "end_future";
  }

  if (
    startedAt !== null &&
    endedAt !== null &&
    problems.start === undefined &&
    problems.duration === undefined
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
    problems.duration === undefined;
  const speedKmh =
    timesRight && km !== null && problems.km === undefined && startedAt !== null && endedAt !== null
      ? km / ((endedAt - startedAt) / 3_600_000)
      : null;

  const ok = Object.keys(problems).length === 0 && startedAt !== null && endedAt !== null;
  return {
    problems,
    nextDay,
    speedKmh,
    times: startedAt !== null && endedAt !== null ? { startedAt, endedAt } : null,
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

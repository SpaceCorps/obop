/**
 * What the calendar decides, as pure functions: which fields place a record in time (a start date,
 * the end of a range, a time of day and an end or a duration), which records it hides by default,
 * how a range is cut into the week rows of a month (bars in lanes), and how overlapping events of
 * one day share its column in the week's time grid. Every function reads the D95 hint
 * (`ui.calendar`) first and falls back to the names of the fields.
 */
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import { isEmpty } from "../format.ts";
import { displayOf, stateFieldOf } from "../presentation.ts";
import { rangeEndOf } from "./board.ts";

/** The fields that place a record on the calendar. */
export type CalSpec = {
  /** The day it starts (or happens). */
  start: FieldMeta;
  /** The day a range ends (leave, a trip): the record is a bar from start to end. */
  end?: FieldMeta;
  /** Minutes after midnight it starts at (a time of day, D95 `display = "time_of_day"`). */
  startMinute?: FieldMeta;
  /** Minutes after midnight it ends at. */
  endMinute?: FieldMeta;
  /** How long it lasts in minutes, when there is no end time. */
  duration?: FieldMeta;
  /** The select whose value colours the event (its status by default). */
  color?: FieldMeta;
};

export const dateFields = (fields: readonly FieldMeta[]) => fields.filter((f) => f.kind === "date");

const START = /^(start|starts|start_on|start_date|starts_on|begins_on|day|date|work_date)$|^start_/;

/** The date field a calendar of the model starts on, before the person picks another. */
export function defaultStart(
  model: ModelMeta,
  fields: readonly FieldMeta[],
): FieldMeta | undefined {
  const dates = dateFields(fields);
  const hinted = dates.find((f) => f.name === model.ui?.calendar?.start);
  return (
    hinted ??
    dates.find((f) => f.required && START.test(f.name)) ??
    dates.find((f) => START.test(f.name)) ??
    dates.find((f) => f.required) ??
    dates[0]
  );
}

/**
 * The fields of the calendar on date field `chosen` (the default start when absent). A range end,
 * times of day and a duration belong to the model's own start date only: on any other date field
 * the record is a one-day event without a time.
 */
export function calendarSpec(
  model: ModelMeta,
  fields: readonly FieldMeta[],
  chosen?: string,
): CalSpec | undefined {
  const main = defaultStart(model, fields);
  const start = dateFields(fields).find((f) => f.name === chosen) ?? main;
  if (!start) return undefined;
  const hint = model.ui?.calendar;
  const byName = (n: string | undefined) => (n ? fields.find((f) => f.name === n) : undefined);
  const stateF = stateFieldOf(model);
  const color = byName(hint?.color) ?? byName(stateF?.name);
  const spec: CalSpec = { start, ...(color ? { color } : {}) };
  if (start !== main) return spec;
  const hintedEnd = byName(hint?.end);
  const end = hintedEnd?.kind === "date" ? hintedEnd : rangeEndOf(fields, start);
  if (end && end.kind === "date") return { ...spec, end };
  const times = fields.filter((f) => displayOf(f) === "time_of_day");
  const startMinute = times.find((f) => /(^|_)(start|from|begin)/.test(f.name)) ?? times[0];
  if (!startMinute) return spec;
  const endMinute = times.find((f) => f !== startMinute && /(^|_)(end|to|until)/.test(f.name));
  const duration = endMinute ? undefined : fields.find((f) => displayOf(f) === "duration_minutes");
  return {
    ...spec,
    startMinute,
    ...(endMinute ? { endMinute } : {}),
    ...(duration ? { duration } : {}),
  };
}

/** Whether the spec has times of day: the week is then a time grid. */
export const timed = (s: CalSpec | undefined): boolean => !!s?.startMinute;

// ---- days as numbers ---------------------------------------------------------------------------

const DAY = /^\d{4}-\d{2}-\d{2}$/;

/** Days since 1970-01-01 of a `YYYY-MM-DD`. */
export function dayNum(iso: string): number {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return Math.round(Date.UTC(y, m - 1, d) / 86_400_000);
}

/** The `YYYY-MM-DD` of a day number. */
export function isoOfDay(n: number): string {
  return new Date(n * 86_400_000).toISOString().slice(0, 10);
}

export const addDaysIso = (iso: string, n: number) => isoOfDay(dayNum(iso) + n);

// ---- events ------------------------------------------------------------------------------------

/** One record placed in time. */
export type CalEvent = {
  row: Row;
  /** First and last day, `YYYY-MM-DD` (the same for a one-day event). */
  start: string;
  end: string;
  /** Minutes after midnight, when the spec has times. */
  startMin?: number;
  endMin?: number;
};

const minutes = (v: unknown): number | undefined => {
  const n = typeof v === "number" ? v : Number(v);
  return isEmpty(v) || !Number.isFinite(n) ? undefined : Math.max(0, Math.min(1440, n));
};

/** The event of a record, or null when it has no valid start date. */
export function eventOf(spec: CalSpec, row: Row): CalEvent | null {
  const start = String(row[spec.start.name] ?? "").slice(0, 10);
  if (!DAY.test(start)) return null;
  const rawEnd = spec.end ? String(row[spec.end.name] ?? "").slice(0, 10) : "";
  const end = DAY.test(rawEnd) && rawEnd >= start ? rawEnd : start;
  const ev: CalEvent = { row, start, end };
  if (spec.startMinute) {
    const s = minutes(row[spec.startMinute.name]);
    if (s !== undefined) {
      ev.startMin = s;
      const e = spec.endMinute
        ? minutes(row[spec.endMinute.name])
        : spec.duration
          ? (() => {
              const d = minutes(row[spec.duration!.name]);
              return d === undefined ? undefined : Math.min(1440, s + d);
            })()
          : undefined;
      ev.endMin = e !== undefined && e > s ? e : Math.min(1440, s + 60);
    }
  }
  return ev;
}

/** Status words of records that did not or will not happen: hidden until the person shows them. */
const GONE = new Set([
  "cancelled",
  "canceled",
  "refused",
  "rejected",
  "discarded",
  "declined",
  "withdrawn",
  "revoked",
]);

/** Whether the calendar hides a record by default: its status says it is off. */
export function isGone(model: ModelMeta, row: Row): boolean {
  const s = stateFieldOf(model);
  return !!s && GONE.has(String(row[s.name] ?? "").toLowerCase());
}

/** "09:30" from minutes after midnight. */
export function hhmm(min: number): string {
  const h = Math.floor(min / 60) % 24;
  const m = Math.round(min % 60);
  return `${String(min >= 1440 ? 24 : h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

/**
 * A time of day the way the region writes it ("14:30", "2:30 PM"); `hhmm` where the browser has no
 * such format. Midnight at the end of a day is "24:00" in either case.
 */
export function clockOf(min: number, locale: string): string {
  if (min >= 1440) return hhmm(1440);
  try {
    return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(
      new Date(2000, 0, 1, Math.floor(min / 60), Math.round(min % 60)),
    );
  } catch {
    return hhmm(min);
  }
}

// ---- the month: bars in lanes ------------------------------------------------------------------

/** The part of an event inside one week row, on lane `lane` (0 is the top). */
export type Segment = {
  ev: CalEvent;
  /** Columns 0..6 of the week. */
  from: number;
  to: number;
  lane: number;
  /** The event begins or ends inside this week (else it continues across the edge). */
  head: boolean;
  tail: boolean;
};

/**
 * Cut the events into the week that starts on `weekStart` and give each piece the first lane free
 * on all of its days: longer pieces first, then earlier times, so a leave of two weeks is one
 * straight bar on top and the day's appointments sit under it.
 */
export function weekSegments(events: readonly CalEvent[], weekStart: string): Segment[] {
  const w0 = dayNum(weekStart);
  const pieces: Omit<Segment, "lane">[] = [];
  for (const ev of events) {
    const s = dayNum(ev.start) - w0;
    const e = dayNum(ev.end) - w0;
    if (e < 0 || s > 6) continue;
    pieces.push({ ev, from: Math.max(0, s), to: Math.min(6, e), head: s >= 0, tail: e <= 6 });
  }
  pieces.sort(
    (a, b) =>
      a.from - b.from ||
      b.to - b.from - (a.to - a.from) ||
      (a.ev.startMin ?? -1) - (b.ev.startMin ?? -1) ||
      a.ev.row.id.localeCompare(b.ev.row.id),
  );
  const lanes: boolean[][] = [];
  return pieces.map((p) => {
    let lane = 0;
    for (; ; lane++) {
      const used = (lanes[lane] ??= []);
      let free = true;
      for (let c = p.from; c <= p.to; c++) if (used[c]) free = false;
      if (free) {
        for (let c = p.from; c <= p.to; c++) used[c] = true;
        break;
      }
    }
    return { ...p, lane };
  });
}

// ---- the week: a time grid ---------------------------------------------------------------------

/** An event of one day in the time grid, with the share of the day's column it gets. */
export type Placed = { ev: CalEvent; col: number; cols: number };

/** Events that did not fit beside the others: one "+n" block in the last column, over their time. */
export type Overflow = { events: CalEvent[]; col: number; cols: number; from: number; to: number };

/** At most this many events stand side by side; the rest of a crowded hour is one "+n" block. */
export const MAX_COLS = 4;

/**
 * Overlapping events of one day side by side: each group of events that overlap one another is
 * split into as many columns as it needs, and each event takes the first column free at its start.
 * A group that needs more than `max` columns keeps `max - 1` of them and gathers the events of the
 * others into one overflow block in the last column.
 */
export function placeDay(
  events: readonly CalEvent[],
  max = MAX_COLS,
): { placed: Placed[]; overflow: Overflow[] } {
  const sorted = [...events]
    .filter((e) => e.startMin !== undefined)
    .sort((a, b) => a.startMin! - b.startMin! || b.endMin! - a.endMin!);
  const placed: Placed[] = [];
  const overflow: Overflow[] = [];
  let group: Placed[] = [];
  let groupEnd = -1;
  const colEnds: number[] = [];
  const flush = () => {
    const cols = Math.max(1, ...group.map((g) => g.col + 1));
    if (cols <= max) for (const g of group) placed.push({ ...g, cols });
    else {
      const rest = group.filter((g) => g.col >= max - 1).map((g) => g.ev);
      for (const g of group) if (g.col < max - 1) placed.push({ ...g, cols: max });
      overflow.push({
        events: rest,
        col: max - 1,
        cols: max,
        from: Math.min(...rest.map((e) => e.startMin!)),
        to: Math.max(...rest.map((e) => e.endMin!)),
      });
    }
    group = [];
    colEnds.length = 0;
  };
  for (const ev of sorted) {
    if (ev.startMin! >= groupEnd && group.length) flush();
    let col = colEnds.findIndex((end) => end <= ev.startMin!);
    if (col === -1) col = colEnds.length;
    colEnds[col] = ev.endMin!;
    group.push({ ev, col, cols: 1 });
    groupEnd = Math.max(groupEnd, ev.endMin!);
  }
  if (group.length) flush();
  return { placed, overflow };
}

/** The hours the grid opens on: from an hour before the first event to an hour after the last. */
export function visibleHours(events: readonly CalEvent[]): { from: number; to: number } {
  const timedEvs = events.filter((e) => e.startMin !== undefined);
  if (!timedEvs.length) return { from: 8, to: 18 };
  const first = Math.min(...timedEvs.map((e) => e.startMin!));
  const last = Math.max(...timedEvs.map((e) => e.endMin!));
  return {
    from: Math.max(0, Math.min(8, Math.floor(first / 60) - 1)),
    to: Math.min(24, Math.max(18, Math.ceil(last / 60) + 1)),
  };
}

/** Round minutes to the grid's step (15 minutes) and keep them inside the day. */
export const snap = (min: number, step = 15) =>
  Math.max(0, Math.min(1440, Math.round(min / step) * step));

/**
 * The fields to write when an event moves to `day` and `startMin`..`endMin`: the date, the start
 * time, and the end time or the duration, whichever the spec has.
 */
export function movePatch(
  spec: CalSpec,
  day: string,
  startMin: number,
  endMin: number,
): Record<string, unknown> {
  const patch: Record<string, unknown> = { [spec.start.name]: day };
  if (spec.startMinute) patch[spec.startMinute.name] = startMin;
  if (spec.endMinute) patch[spec.endMinute.name] = endMin;
  else if (spec.duration) patch[spec.duration.name] = endMin - startMin;
  return patch;
}

/** Whether every field a move writes may be written (not computed, not read-only). */
export function movable(spec: CalSpec | undefined): boolean {
  if (!spec?.startMinute) return false;
  return [spec.start, spec.startMinute, spec.endMinute, spec.duration]
    .filter((f): f is FieldMeta => !!f)
    .every((f) => !f.computed && !f.readonly && !f.sequence);
}

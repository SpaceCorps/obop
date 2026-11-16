/**
 * The model of the week grid (GridView): which fields make a grid, the days of a week, the lines
 * (one per record of the row field, and per owner when several people's entries are shown) and
 * what typing a cell means as a command. Pure functions, no React.
 *
 * A model fits the grid when it has a ref (the line: a task), a date (the column: the day) and a
 * number shown as a duration (the cell: minutes). Timesheets is the first; nothing here names it.
 */
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import { displayOf, stateFieldOf } from "../presentation.ts";
import { addDays } from "../filters.ts";
import { parseDuration } from "../inputs/display.ts";

export type GridSpec = {
  /** The ref each line stands for (`task_id`). */
  line: FieldMeta;
  /** The day of an entry (`worked_on`). */
  day: FieldMeta;
  /** The minutes of an entry (`minutes`). */
  value: FieldMeta;
  /** The person an entry belongs to (`[model.access] own`), when the model says. */
  owner?: FieldMeta;
  /** The status field, when the model has one: only entries in its first option are typed over. */
  state?: FieldMeta;
};

const writable = (f: FieldMeta) => !f.computed && !f.sequence && !f.readonly;

/** The grid of `model` over the fields this person sees, or null when the model has no such three. */
export function gridSpecOf(model: ModelMeta, fields: readonly FieldMeta[]): GridSpec | null {
  const by = (n: string | undefined) => (n ? fields.find((f) => f.name === n) : undefined);
  const calStart = by(model.ui?.calendar?.start);
  const day =
    (calStart?.kind === "date" ? calStart : undefined) ??
    fields.find((f) => f.kind === "date" && f.required && writable(f)) ??
    fields.find((f) => f.kind === "date" && writable(f));
  const value = fields.find(
    (f) =>
      (f.kind === "number" || f.kind === "decimal") &&
      writable(f) &&
      displayOf(f) === "duration_minutes",
  );
  const line =
    fields.find((f) => f.kind === "ref" && f.required && writable(f)) ??
    fields.find((f) => f.kind === "ref" && writable(f));
  if (!day || !value || !line) return null;
  const owner = by(model.access?.own ?? undefined);
  const state = stateFieldOf(model);
  return {
    line,
    day,
    value,
    ...(owner ? { owner } : {}),
    ...(state && state.options?.length ? { state } : {}),
  };
}

/** The Monday of the week `day` (YYYY-MM-DD) falls in. */
export function weekStart(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  const back = (d.getUTCDay() + 6) % 7;
  return addDays(day, -back);
}

/** The seven days of the week that starts on `monday`. */
export function weekDays(monday: string): string[] {
  return Array.from({ length: 7 }, (_, i) => addDays(monday, i));
}

export type GridLine = {
  /** `<line id>|<owner>`: stable across renders. */
  key: string;
  ref: string;
  owner: string;
  /** The entries of each day. */
  cells: Record<string, Row[]>;
  total: number;
};

export type Grid = { lines: GridLine[]; dayTotals: Record<string, number>; total: number };

export const minutesOf = (r: Row, spec: GridSpec): number => {
  const n = Number(r[spec.value.name]);
  return Number.isFinite(n) ? n : 0;
};

/**
 * The lines of a week: every line of `rows` that falls on one of `days`, plus the `extra` lines
 * added by hand that have no entry yet. Lines keep the order they are first met in, extra lines last.
 */
export function buildGrid(
  rows: readonly Row[],
  spec: GridSpec,
  days: readonly string[],
  extra: readonly { ref: string; owner: string }[] = [],
): Grid {
  const lines = new Map<string, GridLine>();
  const dayTotals: Record<string, number> = Object.fromEntries(days.map((d) => [d, 0]));
  let total = 0;
  const lineOf = (ref: string, owner: string) => {
    const key = `${ref}|${owner}`;
    let l = lines.get(key);
    if (!l) {
      l = { key, ref, owner, cells: Object.fromEntries(days.map((d) => [d, []])), total: 0 };
      lines.set(key, l);
    }
    return l;
  };
  for (const r of rows) {
    const day = String(r[spec.day.name] ?? "").slice(0, 10);
    if (!(day in dayTotals)) continue;
    const ref = String(r[spec.line.name] ?? "");
    const owner = spec.owner ? String(r[spec.owner.name] ?? "") : "";
    const l = lineOf(ref, owner);
    const m = minutesOf(r, spec);
    l.cells[day]!.push(r);
    l.total += m;
    dayTotals[day]! += m;
    total += m;
  }
  for (const x of extra) if (x.ref) lineOf(x.ref, x.owner);
  return { lines: [...lines.values()], dayTotals, total };
}

/**
 * What is typed into a cell, in minutes: in a grid of hours a bare number is hours ("8", "1.5",
 * "1,5"), as in a paper timesheet; "1:30", "1h 30", "1.5h" and "90m" read as everywhere else.
 * `null` when it cannot be read; 0 for an empty box is the caller's business.
 */
export function parseCellHours(text: string): number | null {
  const s = text.trim().replace(/,/g, ".");
  if (/^\d+(?:\.\d+)?$/.test(s) || /^\.\d+$/.test(s)) return Math.round(Number(s) * 60);
  return parseDuration(s);
}

/** Whether an entry may still be typed over: no status, or the first one (draft). */
export function isOpen(r: Row, spec: GridSpec): boolean {
  if (!spec.state) return true;
  return r[spec.state.name] === spec.state.options![0];
}

export type CellPlan =
  | { kind: "none" }
  | { kind: "create"; minutes: number }
  | { kind: "update"; id: string; minutes: number }
  | { kind: "delete"; id: string }
  /** More than one entry, or one that is no longer a draft: open it instead of typing. */
  | { kind: "locked" };

/** Whether a cell can be typed in: empty, or one entry that is still open. */
export function cellEditable(entries: readonly Row[], spec: GridSpec): boolean {
  return entries.length === 0 || (entries.length === 1 && isOpen(entries[0]!, spec));
}

/** What typing `minutes` (null: the box was cleared) into a cell with `entries` asks the server to do. */
export function cellPlan(
  entries: readonly Row[],
  minutes: number | null,
  spec: GridSpec,
): CellPlan {
  if (!cellEditable(entries, spec)) return { kind: "locked" };
  const one = entries[0];
  const m = minutes ?? 0;
  if (!one) return m > 0 ? { kind: "create", minutes: m } : { kind: "none" };
  if (m === minutesOf(one, spec)) return { kind: "none" };
  return m > 0 ? { kind: "update", id: one.id, minutes: m } : { kind: "delete", id: one.id };
}

/** The entries of the week that "Submit week" moves on: open ones (drafts), when there is a status. */
export function submittable(grid: Grid, spec: GridSpec): Row[] {
  if (!spec.state) return [];
  return grid.lines.flatMap((l) => Object.values(l.cells).flat()).filter((r) => isOpen(r, spec));
}

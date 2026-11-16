// The searches an app offers without being asked: the open records of a model, the overdue ones,
// the person's own and this month's. The sidebar of a single-model app lists them, and an app's
// overview counts them. Each is a list search (query.ts Cond facets) and the same conditions as
// `<model>.search` filters, so the count on a card is what the list behind it shows.
import { msgx } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row, ServerFilter } from "./api.ts";
import { encodeState, type Cond } from "./query.ts";
import { nextActions, stateFieldOf, summable, isIdLike } from "./presentation.ts";
import { governedNames } from "./workflow.ts";

/** The governed status field of a model (a select an action moves), or undefined. */
export function statusOf(model: ModelMeta): FieldMeta | undefined {
  const s = stateFieldOf(model);
  return s && (s.governed === true || governedNames(model).has(s.name)) ? s : undefined;
}

/**
 * The states in which a record still waits for someone: those from which an action moves it on
 * (the forward step nextActions would make the primary button). A record that is done, posted,
 * won, lost or cancelled has no such step, so it is not open.
 */
export function openStates(model: ModelMeta): string[] {
  const s = statusOf(model);
  if (!s) return [];
  return (s.options ?? []).filter(
    (v) => !!nextActions(model, { id: "", [s.name]: v } as Row).primary,
  );
}

/** A date by which a record should be done (due_on, deadline, expires_on, expected_on, ...). */
const DUE = /^(due_on|due_date|deadline|expires_on|valid_until|expected_on|close_on|end_on)$/;
export function dueFieldOf(model: ModelMeta): FieldMeta | undefined {
  return model.fields.find((f) => f.kind === "date" && DUE.test(f.name));
}

/**
 * The text field that names the person a record belongs to: the field the row rules of D58 use
 * (`access.own`), else a field named for a person (assignee, responsible, employee, ...). Holds a
 * user name, so it can be compared with the person signed in.
 */
const PERSON =
  /^(assignee|assigned_to|responsible|owner|employee|driver|worker|requested_by|user|salesperson|technician)$/;
export function personFieldOf(model: ModelMeta): FieldMeta | undefined {
  const own = model.access?.own;
  const byRule = own ? model.fields.find((f) => f.name === own && f.kind === "text") : undefined;
  return byRule ?? model.fields.find((f) => f.kind === "text" && PERSON.test(f.name));
}

/** The date a record is filed under: the first required date, else the first date. */
export function mainDateOf(model: ModelMeta): FieldMeta | undefined {
  const dates = model.fields.filter((f) => f.kind === "date" && !isIdLike(f));
  return dates.find((f) => f.required) ?? dates[0];
}

/** The money field an overview sums: the first one that adds up (not a price or a rate). */
export function mainMoneyOf(model: ModelMeta): FieldMeta | undefined {
  return (
    model.fields.find((f) => f.kind === "money" && !f.computed && summable(f, model)) ??
    model.fields.find((f) => f.kind === "money" && summable(f, model))
  );
}

/** Today in the browser's zone, `YYYY-MM-DD`. */
export function today(now = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

/** The day before `day`. */
export function dayBefore(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - 1);
  return d.toISOString().slice(0, 10);
}

export type QuickFilter = {
  key: "open" | "overdue" | "mine" | "month";
  label: string;
  /** The list search (`st` of the route is `encodeState({ facets, group: [] })`). */
  facets: Cond[];
  /** The same as `<model>.search` filters, for a count. */
  filter: ServerFilter[];
};

/** Message ids in the context `filter`: show them with `tx("filter", label)`. */
const LABELS = {
  open: msgx("filter", "Open"),
  overdue: msgx("filter", "Overdue"),
  mine: msgx("filter", "Mine"),
  month: msgx("filter", "This month"),
};

/**
 * The quick searches of a model, the ones that apply to it: Open (a status with open states that
 * is not all of them), Overdue (a due date before today on an open record), Mine (a person field
 * equal to `user`) and This month (the main date in the current month). Labels are message ids of
 * the context `filter`: show them with `tx("filter", label)`.
 */
export function quickFilters(model: ModelMeta, user: string, day = today()): QuickFilter[] {
  const out: QuickFilter[] = [];
  const s = statusOf(model);
  const open = openStates(model);
  const openFacet: Cond[] = s && open.length ? [{ op: "is", f: s.name, v: open }] : [];
  const openFilter: ServerFilter[] =
    s && open.length
      ? [
          open.length === 1
            ? { field: s.name, op: "eq", value: open[0] }
            : { field: s.name, op: "in", value: open },
        ]
      : [];
  if (s && open.length && open.length < (s.options?.length ?? 0))
    out.push({ key: "open", label: LABELS.open, facets: openFacet, filter: openFilter });
  const due = dueFieldOf(model);
  if (due && openFacet.length) {
    const max = dayBefore(day);
    out.push({
      key: "overdue",
      label: LABELS.overdue,
      facets: [...openFacet, { op: "range", f: due.name, max }],
      filter: [...openFilter, { field: due.name, op: "lte", value: max }],
    });
  }
  const person = personFieldOf(model);
  if (person && user)
    out.push({
      key: "mine",
      label: LABELS.mine,
      facets: [{ op: "is", f: person.name, v: [user] }],
      filter: [{ field: person.name, op: "eq", value: user }],
    });
  const date = mainDateOf(model);
  if (date) {
    const month = day.slice(0, 7);
    const [y, m] = month.split("-").map(Number) as [number, number];
    const next = m === 12 ? `${y + 1}-01-01` : `${y}-${String(m + 1).padStart(2, "0")}-01`;
    out.push({
      key: "month",
      label: LABELS.month,
      facets: [{ op: "month", f: date.name, v: month }],
      filter: [
        { field: date.name, op: "gte", value: `${month}-01` },
        { field: date.name, op: "lt", value: next },
      ],
    });
  }
  return out;
}

/** The `st` of a route that shows a quick filter. */
export const quickState = (q: Pick<QuickFilter, "facets">): string =>
  encodeState({ facets: q.facets, group: [] });

// What the list view shows, decided without React (lane L2 of the UI plan): the columns that fit,
// the roles of a phone card, which cells edit in place, which dates are due, and the order a list
// starts in. Everything reads the presentation engine (presentation.ts), so a D95 hint the server
// sends (`ui.list`, `ui.order`, `ui.aggregate`, `ui.warn_before_days`, `readonly`) wins over the
// heuristics here without a change to this file.
import type { FieldMeta, ModelMeta, Row, ServerOrder } from "../api.ts";
import { isEmpty } from "../format.ts";
import {
  defaultOrder,
  listColumns,
  stateFieldOf,
  summable,
  titleFieldsOf,
  toneOf,
} from "../presentation.ts";
import type { Sort } from "../query.ts";

/** Room the list keeps for the selection column, in pixels. */
const SELECT_COL = 52;

/**
 * The columns of the list at `width` pixels: the person's own choice when they made one (names
 * that no longer exist are dropped), else the presentation engine's ranking for that width.
 */
export function chooseColumns(
  model: ModelMeta,
  fields: FieldMeta[],
  width: number,
  rows: readonly Row[],
  custom: readonly string[] | null,
): FieldMeta[] {
  if (custom && custom.length) {
    const own = custom
      .map((n) => fields.find((f) => f.name === n))
      .filter((f): f is FieldMeta => !!f);
    if (own.length) return own;
  }
  // listColumns keeps 48 px for the selection column itself.
  return listColumns(model, Math.max(0, width - (SELECT_COL - 48)), { fields, rows });
}

/**
 * How the first column draws a record. `title`: the record's whole title (titleOf), because the
 * column is the first field of the title and the rest of the title is not a column of its own.
 * `value`: the field's own value as plain text (a ref is never a link there).
 */
export function firstColumnMode(model: ModelMeta, cols: readonly FieldMeta[]): "title" | "value" {
  const first = cols[0];
  if (!first) return "value";
  const names = titleFieldsOf(model);
  if (names[0] !== first.name) return "value";
  const rest = names.slice(1);
  return rest.length > 0 && rest.every((n) => cols.some((c) => c.name === n)) ? "value" : "title";
}

/** The fields a phone card shows, by role. Every key but `title` may be missing. */
export type CardRoles = {
  /** The first column: the card's title. */
  title?: FieldMeta;
  state?: FieldMeta;
  /** The amount on the right of the second line. */
  money?: FieldMeta;
  /**
   * The candidates for the muted second line, best first: the number, the refs, the dates (a
   * deadline first), then the other fields. `cardMeta` picks what a record has filled in.
   */
  number?: FieldMeta;
  refs: FieldMeta[];
  dates: FieldMeta[];
  others: FieldMeta[];
  /** An image drawn as the card's avatar. */
  image?: FieldMeta;
};

const DUE = /^(due_on|due_date|deadline|expires_on|expires_at|valid_until|expected_on|close_on)$/;

/** A field whose date is a deadline: its name says so, or the field carries `ui.warn_before_days`. */
export function isDueLike(f: FieldMeta): boolean {
  if (f.kind !== "date" && f.kind !== "datetime") return false;
  return f.ui?.warn_before_days !== undefined || DUE.test(f.name);
}

/**
 * The card of a record on a phone: the title in bold with the status on the right, then the
 * number, main ref and date muted with the amount on the right. Chosen from the same ranking the
 * columns use (`ui.list` included), never by position.
 */
export function cardRoles(model: ModelMeta, fields: FieldMeta[], rows: readonly Row[]): CardRoles {
  const ranked = listColumns(model, 100_000, { fields, rows });
  const stateName = stateFieldOf(model)?.name;
  const state = fields.find((f) => f.name === stateName);
  const title = ranked.find((f) => f !== state);
  const titleNames = new Set(titleFieldsOf(model));
  if (title) titleNames.add(title.name);
  const rest = ranked.filter((f) => f !== state && !titleNames.has(f.name));
  const moneys = rest.filter((f) => f.kind === "money");
  const money = moneys.find((f) => summable(f, model)) ?? moneys[0];
  const number = fields.find((f) => f.sequence && !titleNames.has(f.name));
  const isDate = (f: FieldMeta) => f.kind === "date" || f.kind === "datetime";
  const dates = [...rest.filter(isDueLike), ...rest.filter((f) => isDate(f) && !isDueLike(f))];
  const refs = rest.filter((f) => f.kind === "ref");
  const others = rest.filter(
    (f) => f !== money && f !== number && f.kind !== "ref" && !isDate(f) && f.kind !== "bool",
  );
  const image = fields.find((f) => f.kind === "image" && f.class !== "special_category");
  return {
    ...(title ? { title } : {}),
    ...(state ? { state } : {}),
    ...(money ? { money } : {}),
    ...(number ? { number } : {}),
    refs,
    dates,
    others,
    ...(image ? { image } : {}),
  };
}

/**
 * The muted second line of one record's card, at most three parts: its number, its first ref that
 * is filled in, its first date that is filled in (a deadline first); a record with fewer than two
 * of those (a contact) shows its next filled fields instead.
 */
export function cardMeta(roles: CardRoles, row: Row): FieldMeta[] {
  const set = (f: FieldMeta | undefined): f is FieldMeta => !!f && !isEmpty(row[f.name]);
  const out = [roles.number, roles.refs.find(set), roles.dates.find(set)].filter(set);
  for (const f of roles.others) {
    if (out.length >= 2) break;
    if (set(f)) out.push(f);
  }
  return out;
}

/**
 * Whether the list adds a column up: presentation.ts `summable`, except that a count of days on
 * master data (payment terms, lead times of a vendor) is a setting, not an amount of something.
 */
export function listSummable(f: FieldMeta, model: ModelMeta): boolean {
  if (!f.ui?.aggregate && f.kind === "number" && f.name.endsWith("_days") && isMasterData(model))
    return false;
  return summable(f, model);
}

/** A model without a workflow and without a required date: contacts, products, vendors, accounts. */
export function isMasterData(model: ModelMeta): boolean {
  const state = stateFieldOf(model);
  const workflow =
    !!state && (state.governed === true || model.actions.some((a) => a.field === state.name));
  const dated = model.fields.some(
    (f) => (f.kind === "date" || f.kind === "datetime") && f.required,
  );
  return !workflow && !dated;
}

/**
 * The columns without the `n` lowest-ranked ones that may go: the first column, the first amount
 * and the status (last) always stay. ListView drops one more while the table is wider than its box,
 * since the widths presentation.ts estimates cannot know the words in the cells.
 */
export function trimColumns(cols: readonly FieldMeta[], n: number, state?: string): FieldMeta[] {
  const keep = new Set<FieldMeta>();
  if (cols[0]) keep.add(cols[0]);
  const money = cols.find((f) => f.kind === "money");
  if (money) keep.add(money);
  const last = cols.at(-1);
  if (last && last.name === state) keep.add(last);
  const out = [...cols];
  for (let i = out.length - 1, left = n; i >= 0 && left > 0; i--)
    if (!keep.has(out[i]!)) {
      out.splice(i, 1);
      left--;
    }
  return out;
}

/** How many columns `trimColumns` may still take away. */
export const droppable = (cols: readonly FieldMeta[], state?: string) =>
  cols.length - trimColumns(cols, cols.length, state).length;

/** The base width of a column by kind, in pixels: wide enough for its usual values. */
const KIND_WIDTH: Partial<Record<FieldMeta["kind"], number>> = {
  ref: 180,
  date: 124,
  datetime: 168,
  money: 136,
  decimal: 128,
  number: 104,
  bool: 88,
  select: 140,
  text: 176,
  longtext: 220,
  image: 72,
  file: 168,
};
/** The first column's base width for a name; it also takes the larger share of what is left. */
export const TITLE_WIDTH = 280;
/** Room the selection column takes (list.css .list .selcol). */
const SELECT_WIDTH = 52;

/**
 * A column's width in pixels: its kind's, widened for a long head (about 7.2 px a character plus
 * the padding and the sort mark), at most 260. The table is laid out fixed with these widths, and
 * the browser shares any room left over among the columns in proportion.
 */
export function columnWidth(f: FieldMeta, head: string, first = false): number {
  // A number or a code is short ("SO-00027"); a name or a description is long.
  const short = f.sequence || f.name === "code";
  if (first) return short ? 160 : f.kind === "ref" ? 200 : TITLE_WIDTH;
  const base = short ? 150 : (KIND_WIDTH[f.kind] ?? 160);
  return Math.round(Math.min(260, Math.max(base, head.length * 7.2 + 44)));
}

/**
 * The columns that fit `width` pixels and their widths: the lowest-ranked go first (trimColumns),
 * until the rest fit or only the title, the first amount and the status are left. What is left
 * over is shared out, so the widths fill `width` exactly (an infinite width shares nothing).
 */
export function fitColumns(
  ranked: readonly FieldMeta[],
  width: number,
  head: (f: FieldMeta) => string,
  state?: string,
): { cols: FieldMeta[]; widths: number[] } {
  const widthsOf = (cs: readonly FieldMeta[]) => cs.map((f, i) => columnWidth(f, head(f), i === 0));
  let cols = [...ranked];
  let widths = widthsOf(cols);
  const total = () => widths.reduce((a, b) => a + b, SELECT_WIDTH);
  while (total() > width && droppable(cols, state) > 0) {
    cols = trimColumns(cols, 1, state);
    widths = widthsOf(cols);
  }
  // Room left over: half goes to the title (at most 400 px more), the rest to every column in
  // proportion, so a short list of columns spreads out evenly instead of leaving a gap.
  const extra = Number.isFinite(width) ? Math.floor(width - total()) : 0;
  if (extra > 0 && widths.length) {
    const toTitle = widths.length === 1 ? extra : Math.min(Math.floor(extra / 2), 400);
    const rest = extra - toTitle;
    const others = widths.slice(1).reduce((a, b) => a + b, 0);
    widths = widths.map((w, i) =>
      i === 0 ? w + toTitle : w + (others ? Math.floor((rest * w) / others) : 0),
    );
  }
  return { cols, widths };
}

/** Whether a cell may turn into an input: not a number the server gives, a computed or read-only field. */
export function editableField(f: FieldMeta): boolean {
  if (f.computed || f.sequence || f.readonly) return false;
  return f.kind !== "image" && f.kind !== "file";
}

/** A masked value (`IBAN_3`): the person sees a placeholder, never the value, so it is not edited. */
export const isMaskedValue = (v: unknown) =>
  typeof v === "string" && /^[A-Z][A-Z0-9]*_\d+$/.test(v);

/** States after which a deadline no longer matters. */
const SETTLED = new Set(["archived", "superseded", "discarded", "reversed"]);

/**
 * Whether a due-like date is past (`past`), close (`soon`: within `ui.warn_before_days`, default
 * 7 days), or neither. A record whose status is settled (done, paid, cancelled, expired, archived)
 * has no deadline left. `today` is `YYYY-MM-DD` in the instance's zone; `now` an instant.
 */
export function dueLevel(
  model: ModelMeta,
  f: FieldMeta,
  row: Row,
  today: string,
  now: string = new Date().toISOString(),
): "past" | "soon" | null {
  if (!isDueLike(f)) return null;
  const v = row[f.name];
  if (isEmpty(v) || typeof v !== "string") return null;
  const state = stateFieldOf(model);
  if (state) {
    const s = String(row[state.name] ?? "");
    const tone = toneOf(state, s, model);
    if (tone === "ok" || tone === "bad" || SETTLED.has(s)) return null;
  }
  const days = f.ui?.warn_before_days ?? 7;
  if (f.kind === "datetime") {
    if (v < now) return "past";
    const soon = new Date(Date.parse(now) + days * 86_400_000).toISOString();
    return v <= soon ? "soon" : null;
  }
  const day = v.slice(0, 10);
  if (day < today) return "past";
  return day <= addDays(today, days) ? "soon" : null;
}

function addDays(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/**
 * The order the list is in: the person's sort when they chose one, else the model's default order
 * (`ui.order`, else the main date newest first, the number highest first, the id newest first).
 * Fields the person cannot read are left out; "id" always can be.
 */
export function effectiveOrder(
  model: ModelMeta,
  fields: readonly FieldMeta[],
  sort: Sort | undefined,
): ServerOrder[] {
  if (sort) return [{ field: sort.f, dir: sort.dir }];
  return listOrder(model, fields);
}

/** Classes an agent's token may not sort by (the server refuses a sort it could not read in clear). */
const UNSORTABLE = new Set(["secret", "confidential_hr", "special_category"]);

/**
 * The default order of a list, restricted to the fields this person can read and the server can
 * sort. presentation.ts `defaultOrder` decides (`ui.order`, the main date, the number, the id);
 * where it has nothing but the id, a model without a workflow and without a date is master data
 * (contacts, products, accounts, warehouses) and is listed by its code, else by its name, as people
 * look such lists up; records with a workflow stay newest first.
 */
export function listOrder(model: ModelMeta, fields: readonly FieldMeta[]): ServerOrder[] {
  const sortable = (f: FieldMeta) =>
    f.kind !== "ref" && !f.personal_data && !UNSORTABLE.has(f.class);
  const ok = (name: string) => name === "id" || fields.some((f) => f.name === name && sortable(f));
  const base = defaultOrder(model);
  const onlyId = !model.ui?.order && base.length === 1 && base[0]?.field === "id";
  if (onlyId) {
    if (isMasterData(model)) {
      const text = (n: string) =>
        fields.find((f) => f.name === n && f.kind === "text" && sortable(f));
      const by =
        text("code") ??
        titleFieldsOf(model)
          .map(text)
          .find((f) => !!f);
      if (by) return [{ field: by.name, dir: "asc" }];
    }
  }
  const out = base.filter((o) => ok(o.field));
  return out.length ? out : [{ field: "id", dir: "desc" }];
}

/**
 * The next sort when a column head is clicked. Without a sort of their own the list is in its
 * default order, so the first click on the default column turns that order round; after that a
 * column goes ascending, descending, and back to the default.
 */
export function nextSort(
  current: Sort | undefined,
  field: string,
  fallback: readonly ServerOrder[],
): Sort | undefined {
  if (!current) {
    const d = fallback[0];
    if (d && d.field === field) return { f: field, dir: d.dir === "asc" ? "desc" : "asc" };
    return { f: field, dir: "asc" };
  }
  if (current.f !== field) return { f: field, dir: "asc" };
  return current.dir === "asc" ? { f: field, dir: "desc" } : undefined;
}

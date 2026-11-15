/**
 * What the pivot, the graph, the dashboard tiles and the app overviews add up and how they say it
 * (the L4 lane of the UI plan, section 2.5). Pure functions over `/api/meta` and the server's
 * aggregate answer (D4); no React, no fetching. `useAggregate.ts` fetches, `charts/` draws.
 *
 * - A dimension is a field to group by, and for a date or a datetime a period (day, week, month,
 *   year; D72). It is written `field` or `field:bucket` in the address and in the `ui.report` hint
 *   (`rows = "service_on:month"`); a plain date field means its month, as it always has.
 * - A measure is the record count (`null`), the sum of a field, or its average. It is written
 *   `count`, `field` or `avg:field`. Only fields that `presentation.summable` adds up are offered as
 *   sums; averages are offered for fields marked `aggregate = "avg"`, for percentages and for unit
 *   prices, rates and costs. An average is the sum over the records that have a value divided by
 *   their number, so the margins of a pivot stay exact.
 * - Money in different currencies is never added together: a report keeps one value per currency
 *   (B11) and the views draw one column (pivot) or one chart (graph) per currency.
 */
import { fmt, getCurrency, t } from "@obop/ui";
import type {
  AggGroup,
  AggGroupBy,
  AggMeasure,
  FieldMeta,
  ModelMeta,
  Row,
  ServerFilter,
} from "./api.ts";
import { integer, isEmpty, monthLabel, money } from "./format.ts";
import { dayInZone } from "./instants.ts";
import { fieldTitle } from "./labels.ts";
import {
  childrenOf,
  displayOf,
  isIdLike,
  stateFieldOf,
  summable,
  type MetaIndex,
} from "./presentation.ts";
import { condForKey, groupLabel, orderKeys, type Cond, type Lookup } from "./query.ts";

/** The string key the views use for a group value: "" for empty, "true"/"false" for a bool. */
export function keyOf(v: unknown): string {
  if (v === null || v === undefined || v === "") return "";
  if (typeof v === "boolean") return v ? "true" : "false";
  return String(v);
}

// ---- dimensions --------------------------------------------------------------------------------

export type Bucket = "day" | "week" | "month" | "year";
export const BUCKETS: readonly Bucket[] = ["day", "week", "month", "year"];
const isBucket = (s: unknown): s is Bucket => BUCKETS.includes(s as Bucket);

/** A field to group by; a date or datetime always has a period. */
export type Dim = { field: FieldMeta; bucket?: Bucket };

const DIM_KINDS = new Set(["select", "ref", "bool", "date", "datetime"]);
export const isDated = (f: Pick<FieldMeta, "kind">) => f.kind === "date" || f.kind === "datetime";

/** The fields a report can group by, in declared order: selects, refs, yes/no, dates, instants. */
export function dimensionsOf(fields: readonly FieldMeta[]): FieldMeta[] {
  return fields.filter((f) => DIM_KINDS.has(f.kind) && !f.ui?.hidden);
}

/** `field` or `field:bucket` as a dimension of these fields; undefined when it names none. */
export function parseDim(
  ref: string | undefined | null,
  dims: readonly FieldMeta[],
): Dim | undefined {
  if (!ref) return undefined;
  const [name, b] = ref.split(":");
  const field = dims.find((f) => f.name === name);
  if (!field) return undefined;
  return isDated(field) ? { field, bucket: isBucket(b) ? b : "month" } : { field };
}

/** The address form of a dimension; a month stays the plain field name (older links read alike). */
export const dimRef = (d: Dim): string =>
  d.bucket && d.bucket !== "month" ? `${d.field.name}:${d.bucket}` : d.field.name;

/** The server's group-by entry of a dimension (D4). */
export const groupByOf = (d: Dim): AggGroupBy =>
  isDated(d.field) ? { field: d.field.name, bucket: d.bucket ?? "month" } : { field: d.field.name };

/** The name of a dimension: "Status", "Order date (week)". */
export function dimTitle(d: Dim): string {
  const name = fieldTitle(d.field);
  if (!isDated(d.field)) return name;
  switch (d.bucket ?? "month") {
    case "day":
      return t("{field} (day)", { field: name });
    case "week":
      return t("{field} (week)", { field: name });
    case "year":
      return t("{field} (year)", { field: name });
    default:
      return t("{field} (month)", { field: name });
  }
}

/** The name of a period choice in a menu. */
export function bucketName(b: Bucket): string {
  switch (b) {
    case "day":
      return t("Day");
    case "week":
      return t("Week");
    case "year":
      return t("Year");
    default:
      return t("Month");
  }
}

const shortDay = (key: string, withYear: boolean) =>
  fmt.date(key, { day: "numeric", month: "short", ...(withYear ? { year: "numeric" } : {}) });

/** A period's label: "14 Nov 2026", "Week of 9 Nov 2026", "November 2026", "2026". */
export function bucketLabel(bucket: Bucket, key: string): string {
  if (!key) return t("None");
  switch (bucket) {
    case "day":
      return shortDay(key, true);
    case "week":
      return t("Week of {date}", { date: shortDay(key, true) });
    case "year":
      return key;
    default:
      return monthLabel(key);
  }
}

/** A group's label under a dimension (a ref's name, an option's label, a period). */
export function dimLabel(d: Dim, key: string, lk: Lookup): string {
  return isDated(d.field) ? bucketLabel(d.bucket ?? "month", key) : groupLabel(d.field, key, lk);
}

/** A shorter label for the axis of a chart: periods without "Week of" and the year. */
export function dimAxisLabel(d: Dim, key: string, lk: Lookup): string {
  if (!isDated(d.field) || !key) return dimLabel(d, key, lk);
  const b = d.bucket ?? "month";
  if (b === "day" || b === "week") return shortDay(key, false);
  return bucketLabel(b, key);
}

const addDays = (day: string, n: number): string => {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10);
};
const lastOfMonth = (ym: string): string => {
  const [y, m] = ym.split("-").map(Number) as [number, number];
  return new Date(Date.UTC(y, m, 0)).toISOString().slice(0, 10);
};

/** The Monday of a day's week (weeks start on Monday, as the server cuts them). */
export function mondayOf(day: string): string {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  return addDays(day, -((wd + 6) % 7));
}

/** The condition that lists exactly the records of one group (for a drill into the list). */
export function dimCond(d: Dim, key: string): Cond {
  if (!isDated(d.field) || key === "") return condForKey(d.field, key);
  const f = d.field.name;
  switch (d.bucket ?? "month") {
    case "day":
      return { op: "range", f, min: key, max: key };
    case "week":
      return { op: "range", f, min: key, max: addDays(key, 6) };
    case "year":
      return { op: "range", f, min: `${key}-01-01`, max: `${key}-12-31` };
    default:
      // A date's month has its own facet; an instant's month is a range of days in the zone.
      return d.field.kind === "date"
        ? { op: "month", f, v: key }
        : { op: "range", f, min: `${key}-01`, max: lastOfMonth(key) };
  }
}

/** The group key of a record in the browser, the same as the server's (`YYYY-MM-DD` for a week). */
export function dimKeyOf(d: Dim, row: Row, zone = "UTC"): string {
  const v = row[d.field.name];
  if (isEmpty(v)) return "";
  if (!isDated(d.field)) {
    if (d.field.kind === "bool") return v ? "true" : "false";
    return String(v);
  }
  const day = d.field.kind === "datetime" ? dayInZone(v, zone) : String(v).slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return "";
  switch (d.bucket ?? "month") {
    case "day":
      return day;
    case "week":
      return mondayOf(day);
    case "year":
      return day.slice(0, 4);
    default:
      return day.slice(0, 7);
  }
}

/**
 * The periods between the first and the last key with none left out, so a line over months does
 * not skip a quiet one. Keys are left as they are when that would be more than `max` periods.
 */
export function fillPeriods(keys: readonly string[], bucket: Bucket, max = 120): string[] {
  const set = keys.filter((k) => k !== "").sort();
  if (set.length < 2) return [...keys];
  const first = set[0]!;
  const last = set[set.length - 1]!;
  const next = (k: string): string => {
    switch (bucket) {
      case "day":
        return addDays(k, 1);
      case "week":
        return addDays(k, 7);
      case "year":
        return String(Number(k) + 1);
      default: {
        const [y, m] = k.split("-").map(Number) as [number, number];
        return m === 12 ? `${y + 1}-01` : `${y}-${String(m + 1).padStart(2, "0")}`;
      }
    }
  };
  const out: string[] = [];
  for (let k = first; k <= last; k = next(k)) {
    out.push(k);
    if (out.length > max) return [...keys];
  }
  return keys.includes("") ? [...out, ""] : out;
}

/** The groups of a dimension in order: option order, yes before no, periods, names. */
export function orderDimKeys(d: Dim, keys: readonly string[], lk: Lookup): string[] {
  if (!isDated(d.field)) return orderKeys(d.field, [...keys], lk);
  const uniq = [...new Set(keys)];
  return [...uniq.filter((k) => k !== "").sort(), ...(uniq.includes("") ? [""] : [])];
}

// ---- measures ----------------------------------------------------------------------------------

/** A sum or an average of a field; the record count is `null`. */
export type Measure = { field: FieldMeta; op: "sum" | "avg" };
export type MeasureSel = Measure | null;
export type MeasureOptions = { sums: FieldMeta[]; avgs: FieldMeta[] };

/** Numbers the server keeps for itself, positions and coordinates: never a measure. */
const NOT_A_MEASURE =
  /(^|_)(seq|sequence|last_seq|max_seq|row_version|expected_points|point_count|lat|lon|lng|latitude|longitude|odometer|odometer_km|position|year|month|day)$/;

function averaged(f: FieldMeta, configuration: boolean): boolean {
  const hint = f.ui?.aggregate;
  if (hint) return hint === "avg";
  if (configuration) return false;
  const display = displayOf(f);
  if (display === "percent" || display === "percent_bp") return true;
  if (display) return false;
  return (f.kind === "money" || f.kind === "decimal") && /(price|rate|cost)/.test(f.name);
}

/**
 * What a report can add up: sums of the fields `summable` adds up, and averages of the fields
 * whose total means nothing but whose mean does (a unit price, a rate, a progress percentage).
 * A configuration model (a tax, a plan) has only the count.
 */
export function measureOptions(
  model: ModelMeta,
  fields: readonly FieldMeta[],
  configuration = false,
): MeasureOptions {
  const numeric = fields.filter(
    (f) =>
      (f.kind === "money" || f.kind === "decimal" || f.kind === "number") &&
      !f.ui?.hidden &&
      !isIdLike(f) &&
      !(NOT_A_MEASURE.test(f.name) && !f.ui?.aggregate),
  );
  const sums = numeric.filter((f) => summable(f, model, configuration));
  const avgs = numeric.filter((f) => !sums.includes(f) && averaged(f, configuration));
  return { sums, avgs };
}

/** The address form of a measure: `count`, `field` (a sum) or `avg:field`. */
export const measureRef = (m: MeasureSel): string =>
  !m ? "count" : m.op === "avg" ? `avg:${m.field.name}` : m.field.name;

/**
 * A measure from its address form: `null` for the count, undefined when the text names nothing on
 * offer (the default then applies). A plain field name of a field that is only averaged (an older
 * link that summed it) is read as its average.
 */
export function parseMeasure(
  ref: string | undefined | null,
  opts: MeasureOptions,
): MeasureSel | undefined {
  if (!ref) return undefined;
  if (ref === "count") return null;
  const avg = ref.startsWith("avg:");
  const name = avg ? ref.slice(4) : ref;
  const sum = opts.sums.find((f) => f.name === name);
  const mean = opts.avgs.find((f) => f.name === name);
  if (avg) return mean ? { field: mean, op: "avg" } : sum ? { field: sum, op: "avg" } : undefined;
  if (sum) return { field: sum, op: "sum" };
  if (mean) return { field: mean, op: "avg" };
  return undefined;
}

/**
 * The measure a report opens with: the `ui.report.measure` hint; else the first money field that is
 * added up; else any other sum (a quantity, a duration, a decimal); else the count.
 */
export function defaultMeasure(
  model: ModelMeta,
  opts: MeasureOptions,
  fields: readonly FieldMeta[] = model.fields,
): MeasureSel {
  const hint = model.ui?.report?.measure;
  if (hint) {
    const read = parseMeasure(hint, opts);
    if (read !== undefined) return read;
    // The hint is the package's word: a number it names is summed even if no heuristic would.
    const f = fields.find((x) => x.name === hint);
    if (f && (f.kind === "money" || f.kind === "decimal" || f.kind === "number"))
      return { field: f, op: "sum" };
  }
  const money = opts.sums.find((f) => f.kind === "money");
  if (money) return { field: money, op: "sum" };
  const other = opts.sums[0];
  return other ? { field: other, op: "sum" } : null;
}

/** Whether a measure is money (and so is split by currency). */
export const isMoney = (m: MeasureSel | undefined): m is Measure => !!m && m.field.kind === "money";

/**
 * The field whose value tells the currency of a money measure: the model's own `currency`, else
 * the first step of the `ui.currency_from` path (`journal_id` of `journal_id.currency`), so the
 * server can group by it. Null for a count, a non-money measure, or a model with neither (its
 * money is in the instance's currency).
 */
export function currencySplit(
  model: ModelMeta,
  m: MeasureSel | undefined,
): { field: string; path?: string } | null {
  if (!isMoney(m)) return null;
  if (model.fields.some((f) => f.name === "currency" && (f.kind === "text" || f.kind === "select")))
    return { field: "currency" };
  const path =
    m.field.ui?.currency_from ??
    model.fields.find((f) => f.kind === "money" && f.ui?.currency_from)?.ui?.currency_from;
  const hop = path?.split(".")[0];
  const ref = hop ? model.fields.find((f) => f.name === hop && f.kind === "ref") : undefined;
  return ref ? { field: ref.name, path } : null;
}

/** The measures to ask the server for: always the count, and the sum of the field. */
export const measureAggs = (m: MeasureSel): AggMeasure[] =>
  m ? [{ op: "count" }, { op: "sum", field: m.field.name }] : [{ op: "count" }];

/** An average counts only the records that have a value. */
export const measureFilter = (m: MeasureSel): ServerFilter[] =>
  m?.op === "avg" ? [{ field: m.field.name, op: "is_null", value: false }] : [];

/** The summed value of a server group; a decimal's exact sum arrives as text (D72). */
export function groupSum(g: AggGroup, m: MeasureSel): number {
  if (!m) return g.count;
  const v = g.measures[`sum:${m.field.name}`] as unknown;
  const n = typeof v === "string" ? Number(v) : typeof v === "number" ? v : 0;
  return Number.isFinite(n) ? n : 0;
}

/** A cell of a report: how many records and the sum of the measure over them. */
export type Cell = { count: number; value: number };

/** What a cell shows: the count, the sum, or the average (sum over count). */
export function cellValue(m: MeasureSel, c: Cell | undefined): number {
  if (!c) return 0;
  if (!m) return c.count;
  return m.op === "avg" ? (c.count ? c.value / c.count : 0) : c.value;
}

/** "Count", "Sum of Total", "Average of Unit price". */
export function measureTitle(m: MeasureSel): string {
  if (!m) return t("Count");
  return m.op === "avg"
    ? t("Average of {field}", { field: fieldTitle(m.field) })
    : t("Sum of {field}", { field: fieldTitle(m.field) });
}

const unit = (n: number, u: string, digits = 1) =>
  fmt.number(n, { style: "unit", unit: u, unitDisplay: "short", maximumFractionDigits: digits });

/** A value of a measure as a person reads it: money in its currency, hours, km, a percentage. */
export function formatMeasure(m: MeasureSel, n: number, currency?: string): string {
  if (!m) return integer(Math.round(n));
  const f = m.field;
  if (f.kind === "money") return money(Math.round(n), currency ?? getCurrency());
  switch (displayOf(f)) {
    case "duration_minutes":
      return unit(n / 60, "hour");
    case "km_from_m":
      return unit(n / 1000, "kilometer");
    case "percent":
      return fmt.percent(n / 100, 1);
    case "percent_bp":
      return fmt.percent(n / 10000, 2);
    default:
      return fmt.number(n, {
        maximumFractionDigits: f.kind === "decimal" ? (f.scale ?? 2) : m.op === "avg" ? 2 : 0,
      });
  }
}

/** The short form for an axis or a label on a mark: "€7.5K", "1.2K", "12 hr". */
export function compactMeasure(m: MeasureSel, n: number, currency?: string): string {
  const compact = { notation: "compact", maximumFractionDigits: 1 } as const;
  if (!m) return fmt.number(n, compact);
  const f = m.field;
  if (f.kind === "money") {
    const cur = currency ?? getCurrency();
    try {
      return new Intl.NumberFormat(fmt.locale, {
        style: "currency",
        currency: cur,
        ...compact,
      }).format(n / 10 ** fmt.exponent(cur));
    } catch {
      return money(Math.round(n), cur);
    }
  }
  switch (displayOf(f)) {
    case "duration_minutes":
      return fmt.number(n / 60, { ...compact, style: "unit", unit: "hour", unitDisplay: "short" });
    case "km_from_m":
      return fmt.number(n / 1000, {
        ...compact,
        style: "unit",
        unit: "kilometer",
        unitDisplay: "short",
      });
    case "percent":
    case "percent_bp":
      return formatMeasure(m, n, currency);
    default:
      return fmt.number(n, compact);
  }
}

// ---- defaults ----------------------------------------------------------------------------------

/**
 * The ref through which a model is the line of another (`move_id` of a journal item). A child with
 * a date of its own (a vehicle's service on a day) is an event, and its parent is a fine group.
 */
function parentLinks(model: ModelMeta, index?: MetaIndex): Set<string> {
  const out = new Set<string>();
  if (!index || model.fields.some(isDated)) return out;
  for (const f of model.fields) {
    if (f.kind !== "ref" || !f.to) continue;
    const parent = index.get(f.to);
    if (parent && childrenOf(parent, index).some((c) => c.model === model.name && c.via === f.name))
      out.add(f.name);
  }
  return out;
}

/**
 * The rows a pivot opens with (and the dimension of a graph): the `ui.report.rows` hint; else a
 * ref to master data (a contact, a product, an account), then a ref to another document (a task,
 * a vehicle), the required one first, never the parent of a line nor the model itself; else the
 * main date by month; else the status; else a ref to set-up data (an SLA, a plan); else the first
 * dimension. `isConfiguration` names set-up models; without `index` every ref is master data.
 */
export function defaultRows(
  model: ModelMeta,
  dims: readonly FieldMeta[],
  index?: MetaIndex,
  isConfiguration: (model: string) => boolean = () => false,
): Dim | undefined {
  const hinted = parseDim(model.ui?.report?.rows, dims);
  if (hinted) return hinted;
  const skip = parentLinks(model, index);
  const refs = dims.filter((f) => f.kind === "ref" && f.to !== model.name && !skip.has(f.name));
  const kind = (f: FieldMeta): "master" | "document" | "setup" => {
    if (!f.to) return "master";
    if (isConfiguration(f.to)) return "setup";
    const target = index?.get(f.to);
    const s = target && stateFieldOf(target);
    return s && (s.governed || target.actions?.some((a) => a.field === s.name))
      ? "document"
      : "master";
  };
  const pick = (k: ReturnType<typeof kind>) => {
    const of = refs.filter((f) => kind(f) === k);
    return of.find((f) => f.required) ?? of[0];
  };
  const ref = pick("master") ?? pick("document");
  if (ref) return { field: ref };
  const dates = dims.filter(isDated);
  const date = dates.find((f) => f.required) ?? dates[0];
  if (date) return { field: date, bucket: "month" };
  const state = stateFieldOf(model);
  const s = state && dims.find((f) => f.name === state.name);
  if (s) return { field: s };
  const setup = pick("setup");
  if (setup) return { field: setup };
  const first = dims[0];
  return first ? parseDim(first.name, dims) : undefined;
}

/** The columns a pivot opens with: the `ui.report.cols` hint, else none. */
export function defaultCols(model: ModelMeta, dims: readonly FieldMeta[]): Dim | undefined {
  return parseDim(model.ui?.report?.cols, dims);
}

// ---- reports -----------------------------------------------------------------------------------

/** One group of a report: its key under each dimension, its currency (money only) and its cell. */
export type ReportRow = { keys: string[]; currency?: string; count: number; value: number };

/** The rows of a report from records in the browser (the fallback when the server cannot filter). */
export function reportFromRows(
  rows: readonly Row[],
  dims: readonly Dim[],
  m: MeasureSel,
  opts: { currencyOf?: (row: Row) => string | undefined; zone?: string } = {},
): ReportRow[] {
  const acc = new Map<string, ReportRow>();
  for (const r of rows) {
    const raw = m ? r[m.field.name] : undefined;
    if (m?.op === "avg" && isEmpty(raw)) continue;
    const keys = dims.map((d) => dimKeyOf(d, r, opts.zone));
    const currency = isMoney(m) ? opts.currencyOf?.(r) : undefined;
    const id = JSON.stringify([keys, currency ?? null]);
    const cell = acc.get(id) ?? { keys, currency, count: 0, value: 0 };
    cell.count += 1;
    cell.value += m ? Number(raw) || 0 : 1;
    acc.set(id, cell);
  }
  return [...acc.values()];
}

/**
 * The rows of a report from the server's groups. `split` names the extra group-by field that tells
 * the currency (`currency`, or the first step of `ui.currency_from`), and `currencyOf` turns its
 * value into a currency; groups that end up in the same currency are added together.
 */
export function reportFromGroups(
  groups: readonly (AggGroup & { split?: unknown })[],
  dims: readonly Dim[],
  m: MeasureSel,
  currencyOf?: (splitValue: unknown) => string | undefined,
): ReportRow[] {
  const acc = new Map<string, ReportRow>();
  for (const g of groups) {
    const keys = dims.map((d) => keyOf(g.key[d.field.name]));
    const currency = isMoney(m) ? currencyOf?.(g.split) : undefined;
    const id = JSON.stringify([keys, currency ?? null]);
    const cell = acc.get(id) ?? { keys, currency, count: 0, value: 0 };
    cell.count += g.count;
    cell.value += groupSum(g, m);
    acc.set(id, cell);
  }
  return [...acc.values()];
}

/** The currencies of a report's rows, the instance's first; `[undefined]` for a non-money report. */
export function currenciesOf(rows: readonly ReportRow[]): (string | undefined)[] {
  const set = new Set(rows.map((r) => r.currency));
  const home = getCurrency();
  const list = [...set].sort((a, b) =>
    a === home ? -1 : b === home ? 1 : String(a ?? "").localeCompare(String(b ?? "")),
  );
  return list.length ? list : [undefined];
}

/** A pivot table: row and column keys, and the cells, margins and grand total per currency. */
export type Matrix = {
  rowKeys: string[];
  colKeys: string[];
  currencies: (string | undefined)[];
  cell: (r: string, c: string, cur: string | undefined) => Cell | undefined;
  rowTotal: (r: string, cur: string | undefined) => Cell;
  colTotal: (c: string, cur: string | undefined) => Cell;
  grand: (cur: string | undefined) => Cell;
};

const k3 = (r: string, c: string, cur: string | undefined) => `${r}\u0001${c}\u0001${cur ?? ""}`;
const add = (m: Map<string, Cell>, k: string, x: ReportRow) => {
  const c = m.get(k) ?? { count: 0, value: 0 };
  c.count += x.count;
  c.value += x.value;
  m.set(k, c);
};

/**
 * Lay a report out as a pivot. Sums and counts add up, so the margins come from the cells; an
 * average's margins are the margins' sum over their count, which is exact.
 */
export function matrixOf(
  rows: readonly ReportRow[],
  dims: readonly [Dim, Dim?],
  lk: Lookup,
  fill = false,
): Matrix {
  const [rowD, colD] = dims;
  const cells = new Map<string, Cell>();
  const rt = new Map<string, Cell>();
  const ct = new Map<string, Cell>();
  const gt = new Map<string, Cell>();
  const rks: string[] = [];
  const cks: string[] = [];
  for (const x of rows) {
    const r = x.keys[0] ?? "";
    const c = colD ? (x.keys[1] ?? "") : "";
    rks.push(r);
    cks.push(c);
    add(cells, k3(r, c, x.currency), x);
    add(rt, k3(r, "", x.currency), x);
    add(ct, k3("", c, x.currency), x);
    add(gt, k3("", "", x.currency), x);
  }
  let rowKeys = orderDimKeys(rowD, rks, lk);
  if (fill && isDated(rowD.field)) rowKeys = fillPeriods(rowKeys, rowD.bucket ?? "month");
  const colKeys = colD ? orderDimKeys(colD, cks, lk) : [""];
  const zero = { count: 0, value: 0 };
  return {
    rowKeys,
    colKeys,
    currencies: currenciesOf(rows),
    cell: (r, c, cur) => cells.get(k3(r, c, cur)),
    rowTotal: (r, cur) => rt.get(k3(r, "", cur)) ?? zero,
    colTotal: (c, cur) => ct.get(k3("", c, cur)) ?? zero,
    grand: (cur) => gt.get(k3("", "", cur)) ?? zero,
  };
}

import { compareText, fmt, getCurrency, t } from "@obop/ui";
import { getTime, type FieldMeta, type Kind, type Row, type ServerOrder } from "./api.ts";
import { compareDecimals, isDecimalText, sumDecimals } from "./decimals.ts";
import { dayInZone } from "./instants.ts";
import {
  decimalText,
  decimalToMinor,
  formatDate,
  integer,
  isEmpty,
  money,
  monthLabel,
  optionLabel,
  plainText,
  rowCurrency,
  validDate,
} from "./format.ts";
import { fieldTitle } from "./labels.ts";

/** One search condition. A list of them is the "facets"; the facets are ANDed. */
export type Cond =
  | { op: "search"; v: string }
  | { op: "contains"; f: string; v: string }
  | { op: "is"; f: string; v: string[] }
  | { op: "isnot"; f: string; v: string[] }
  | { op: "bool"; f: string; v: boolean }
  | { op: "range"; f: string; min?: string; max?: string }
  | { op: "month"; f: string; v: string }
  | { op: "empty"; f: string };

export type Sort = { f: string; dir: "asc" | "desc" };
export type PivotCfg = { row?: string; col?: string; measure?: string };
export type GraphCfg = { dim?: string; measure?: string; type?: "bar" | "line" | "pie" };
export type ViewState = {
  facets: Cond[];
  group: string[];
  sort?: Sort;
  pivot?: PivotCfg;
  graph?: GraphCfg;
};

export const emptyState = (): ViewState => ({ facets: [], group: [] });

/** Look-ups the query code needs from the data layer. */
export type Lookup = {
  /** The label of a record of `model`. */
  refText: (model: string, id: string) => string;
  /** Every id of a model the person can see, for ordering ref columns. */
  refIds: (model: string) => string[];
};

export function encodeState(s: ViewState): string {
  const o: Record<string, unknown> = {};
  if (s.facets.length) o.q = s.facets;
  if (s.group.length) o.g = s.group;
  if (s.sort) o.s = s.sort;
  if (s.pivot && Object.keys(s.pivot).length) o.p = s.pivot;
  if (s.graph && Object.keys(s.graph).length) o.r = s.graph;
  return Object.keys(o).length ? JSON.stringify(o) : "";
}

export function decodeState(raw: string): ViewState {
  if (!raw) return emptyState();
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    return {
      facets: Array.isArray(o.q) ? (o.q as Cond[]) : [],
      group: Array.isArray(o.g) ? (o.g as string[]) : [],
      sort: o.s as Sort | undefined,
      pivot: o.p as PivotCfg | undefined,
      graph: o.r as GraphCfg | undefined,
    };
  } catch {
    return emptyState();
  }
}

export const GROUPABLE: Kind[] = ["select", "ref", "bool", "date"];
export const groupable = (fields: FieldMeta[]) => fields.filter((f) => GROUPABLE.includes(f.kind));
export const measurable = (fields: FieldMeta[]) =>
  fields.filter((f) => f.kind === "number" || f.kind === "money");

/** Records are grouped by this key: the value, or the month for dates, "" for nothing. */
export function groupKey(f: FieldMeta, row: Row): string {
  const v = row[f.name];
  if (isEmpty(v)) return "";
  if (f.kind === "date") return String(v).slice(0, 7);
  if (f.kind === "bool") return v ? "true" : "false";
  return String(v);
}

export function orderKeys(f: FieldMeta, keys: string[], lk: Lookup): string[] {
  const uniq = [...new Set(keys)];
  const rank = (k: string): number => {
    if (k === "") return 1e9;
    if (f.kind === "select") {
      const i = f.options?.indexOf(k) ?? -1;
      return i < 0 ? 1e6 : i;
    }
    if (f.kind === "bool") return k === "true" ? 0 : 1;
    return 0;
  };
  return uniq.sort((a, b) => {
    const d = rank(a) - rank(b);
    if (d) return d;
    if (f.kind === "ref" && f.to) return compareText(lk.refText(f.to, a), lk.refText(f.to, b));
    return compareText(a, b);
  });
}

export function groupLabel(f: FieldMeta, key: string, lk: Lookup): string {
  if (key === "") return t("None");
  switch (f.kind) {
    case "select":
      return optionLabel(key, f);
    case "bool":
      return key === "true" ? t("Yes") : t("No");
    case "date":
      return monthLabel(key);
    case "ref":
      return f.to ? lk.refText(f.to, key) : key;
    default:
      return key;
  }
}

/** Every column a grouped view should show: the full domain for select/bool, what exists otherwise. */
export function groupDomain(f: FieldMeta, rows: Row[], lk: Lookup): string[] {
  return domainOf(
    f,
    rows.map((r) => groupKey(f, r)),
    lk,
  );
}

/** Like `groupDomain`, from the group keys that exist (the server's groups, or the rows' keys). */
export function domainOf(f: FieldMeta, present: string[], lk: Lookup): string[] {
  let all = present;
  if (f.kind === "select") all = [...(f.options ?? []), ...present];
  if (f.kind === "bool") all = ["true", "false", ...present];
  if (f.kind === "ref" && f.to) all = [...lk.refIds(f.to), ...present];
  const keys = orderKeys(f, all, lk);
  // "None" only appears when some record has no value.
  return keys.filter((k) => k !== "" || present.includes(""));
}

export type Group = {
  path: string;
  field: FieldMeta;
  key: string;
  label: string;
  rows: Row[];
  children: Group[];
  depth: number;
};

export function buildGroups(
  rows: Row[],
  levels: FieldMeta[],
  lk: Lookup,
  depth = 0,
  parent = "",
): Group[] {
  const f = levels[depth];
  if (!f) return [];
  const buckets = new Map<string, Row[]>();
  for (const r of rows) {
    const k = groupKey(f, r);
    const b = buckets.get(k);
    if (b) b.push(r);
    else buckets.set(k, [r]);
  }
  return orderKeys(f, [...buckets.keys()], lk).map((key) => {
    const own = buckets.get(key)!;
    const path = `${parent}/${f.name}=${key}`;
    return {
      path,
      field: f,
      key,
      label: groupLabel(f, key, lk),
      rows: own,
      children: buildGroups(own, levels, lk, depth + 1, path),
      depth,
    };
  });
}

export const sumOf = (rows: readonly Row[], field: string) =>
  rows.reduce((n, r) => n + (Number(r[field]) || 0), 0);

/** The exact sum of a decimal column, as the server's text form (D72). */
export const sumDecimalRows = (rows: readonly Row[], field: string): string =>
  sumDecimals(rows.map((r) => r[field]).filter(isDecimalText));

export function formatSum(f: FieldMeta | undefined, n: number, currency?: string): string {
  return f?.kind === "money" ? money(n, currency) : integer(n);
}

/**
 * The total of a column. Money in different currencies is never added together: each currency gets
 * its own subtotal (docs/spec/ux.md 4.5). `currencyFor` names a record's currency (presentation.ts
 * `currencyOf`); without it the record's own `currency` field decides. A record with no currency is
 * in the instance's currency, so it is never a subtotal of its own beside that currency (B7).
 */
export function formatSumRows(
  f: FieldMeta | undefined,
  rows: readonly Row[],
  currencyFor: (row: Row) => string | undefined = rowCurrency,
): string {
  if (!f) return "";
  if (f.kind === "decimal") return decimalText(sumDecimalRows(rows, f.name), f);
  if (f.kind !== "money") return integer(sumOf(rows, f.name));
  return fmt.list(
    sumByCurrency(rows, f.name, currencyFor).map(([c, n]) => money(n, c)),
    "conjunction",
  );
}

/** The sums of a money field per currency, largest group first; an empty currency is the instance's. */
export function sumByCurrency(
  rows: readonly Row[],
  field: string,
  currencyFor: (row: Row) => string | undefined = rowCurrency,
): [string, number][] {
  const byCurrency = new Map<string, { n: number; count: number }>();
  for (const r of rows) {
    const c = currencyFor(r) || getCurrency();
    const b = byCurrency.get(c) ?? { n: 0, count: 0 };
    b.n += Number(r[field]) || 0;
    b.count++;
    byCurrency.set(c, b);
  }
  return [...byCurrency]
    .sort((a, b) => b[1].count - a[1].count)
    .map(([c, b]): [string, number] => [c, b.n]);
}

/**
 * The one currency of a set of records, for charts and pivots that add amounts up. `mixed` is true
 * when the records use more than one, so the view can warn that the sums mix currencies.
 */
export function currencyOf(rows: Row[]): { currency?: string; mixed: boolean } {
  const set = new Set(rows.map((r) => rowCurrency(r)).filter((c): c is string => !!c));
  return { currency: set.size === 1 ? [...set][0] : undefined, mixed: set.size > 1 };
}

/** The condition that selects exactly the records of one group. */
export function condForKey(f: FieldMeta, key: string): Cond {
  if (key === "") return { op: "empty", f: f.name };
  if (f.kind === "bool") return { op: "bool", f: f.name, v: key === "true" };
  if (f.kind === "date") return { op: "month", f: f.name, v: key };
  return { op: "is", f: f.name, v: [key] };
}

export function addFacet(facets: Cond[], c: Cond): Cond[] {
  if (c.op === "is" || c.op === "isnot") {
    const i = facets.findIndex((x) => x.op === c.op && x.f === c.f);
    if (i >= 0) {
      const old = facets[i] as Extract<Cond, { op: "is" | "isnot" }>;
      const v = [...new Set([...old.v, ...c.v])];
      return facets.map((x, j) => (j === i ? { ...old, v } : x));
    }
  }
  if (facets.some((x) => JSON.stringify(x) === JSON.stringify(c))) return facets;
  return [...facets, c];
}

export function toggleOption(facets: Cond[], f: string, value: string): Cond[] {
  const i = facets.findIndex((x) => x.op === "is" && x.f === f);
  if (i < 0) return [...facets, { op: "is", f, v: [value] }];
  const old = facets[i] as Extract<Cond, { op: "is" }>;
  const v = old.v.includes(value) ? old.v.filter((x) => x !== value) : [...old.v, value];
  return v.length
    ? facets.map((x, j) => (j === i ? { ...old, v } : x))
    : facets.filter((_, j) => j !== i);
}

/**
 * A range bound is stored in the address as a canonical decimal ("12.50", a dot, no grouping), so a
 * shared link means the same in every locale; this reads it (an older link may hold a comma).
 */
export function rangeNum(f: FieldMeta, s: string | undefined): number | undefined {
  if (s === undefined || s.trim() === "") return undefined;
  const canonical = s.trim().replace(",", ".");
  if (!/^-?\d+(\.\d+)?$/.test(canonical)) return undefined;
  return f.kind === "money"
    ? (decimalToMinor(canonical, fmt.exponent()) ?? undefined)
    : Number(canonical);
}

/**
 * A decimal range bound as the server's text form, or `undefined`. The address holds it with a
 * dot (a link means the same in every locale); an older one may hold a comma.
 */
export function rangeDecimal(s: string | undefined): string | undefined {
  if (s === undefined || s.trim() === "") return undefined;
  const canonical = s.trim().replace(",", ".");
  return isDecimalText(canonical) ? canonical : undefined;
}

export function matchCond(c: Cond, row: Row, fields: FieldMeta[], lk: Lookup): boolean {
  const text = (f: FieldMeta) => plainText(f, row[f.name], lk.refText).toLowerCase();
  if (c.op === "search") {
    const needle = c.v.toLowerCase();
    return fields.some((f) => text(f).includes(needle));
  }
  const f = fields.find((x) => x.name === c.f);
  if (!f) return true;
  const v = row[f.name];
  switch (c.op) {
    case "contains":
      return text(f).includes(c.v.toLowerCase());
    case "is":
      return c.v.includes(f.kind === "bool" ? String(Boolean(v)) : groupKey(f, row));
    case "isnot":
      return !c.v.includes(groupKey(f, row));
    case "bool":
      return Boolean(v) === c.v;
    case "month":
      return typeof v === "string" && v.startsWith(c.v);
    case "empty":
      return isEmpty(v);
    case "range": {
      if (isEmpty(v)) return false;
      if (f.kind === "date") {
        const d = String(v);
        return (!c.min || d >= c.min) && (!c.max || d <= c.max);
      }
      if (f.kind === "datetime") {
        // Bounds are days of the instance's zone; the instant's day there decides.
        const d = dayInZone(v, getTime().zone);
        return (!c.min || d >= c.min) && (!c.max || d <= c.max);
      }
      if (f.kind === "decimal") {
        const x = String(v);
        const lo = rangeDecimal(c.min);
        const hi = rangeDecimal(c.max);
        return (
          isDecimalText(x) &&
          (lo === undefined || compareDecimals(x, lo) >= 0) &&
          (hi === undefined || compareDecimals(x, hi) <= 0)
        );
      }
      const n = Number(v);
      const lo = rangeNum(f, c.min);
      const hi = rangeNum(f, c.max);
      return (lo === undefined || n >= lo) && (hi === undefined || n <= hi);
    }
  }
}

export function applyFacets(rows: Row[], facets: Cond[], fields: FieldMeta[], lk: Lookup): Row[] {
  return facets.length
    ? rows.filter((r) => facets.every((c) => matchCond(c, r, fields, lk)))
    : rows;
}

function compare(f: FieldMeta, a: Row, b: Row, lk: Lookup): number {
  const x = a[f.name];
  const y = b[f.name];
  switch (f.kind) {
    case "number":
    case "money":
      return (Number(x) || 0) - (Number(y) || 0);
    case "decimal":
      return compareDecimals(String(x), String(y));
    // Instants are fixed-width UTC text: text order is time order.
    case "datetime":
      return String(x) < String(y) ? -1 : String(x) > String(y) ? 1 : 0;
    case "bool":
      return Number(Boolean(x)) - Number(Boolean(y));
    case "select": {
      const o = f.options ?? [];
      return o.indexOf(String(x)) - o.indexOf(String(y));
    }
    default:
      return compareText(plainText(f, x, lk.refText), plainText(f, y, lk.refText));
  }
}

/**
 * Rows in the order of `sort`. Without a sort of the person's own, `fallback` applies (the model's
 * default order, `listOrder` in views/listLayout.ts), so a list loaded into the browser is in the same
 * order as a list the server pages; `id` there orders by the id (UUIDv7, so by creation time).
 */
export function sortRows(
  rows: Row[],
  sort: Sort | undefined,
  fields: FieldMeta[],
  lk: Lookup,
  fallback: readonly ServerOrder[] = [],
) {
  const keys: { f: FieldMeta | "id"; sign: number }[] = [];
  const order = sort ? [{ field: sort.f, dir: sort.dir }] : fallback;
  for (const o of order) {
    const f = o.field === "id" ? "id" : fields.find((x) => x.name === o.field);
    if (f) keys.push({ f, sign: o.dir === "asc" ? 1 : -1 });
  }
  if (!keys.length) return rows;
  return [...rows].sort((a, b) => {
    for (const { f, sign } of keys) {
      if (f === "id") {
        const d = a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
        if (d) return sign * d;
        continue;
      }
      const ea = isEmpty(a[f.name]);
      const eb = isEmpty(b[f.name]);
      if (ea || eb) {
        if (ea !== eb) return Number(ea) - Number(eb); // empty values always last
        continue;
      }
      const d = compare(f, a, b, lk);
      if (d) return sign * d;
    }
    return 0;
  });
}

/** Chip text for a facet: "Status is Open", "Amount 100 to 500". */
export function describeCond(c: Cond, fields: FieldMeta[], lk: Lookup): string {
  if (c.op === "search") return t("Search: {text}", { text: c.v });
  const f = fields.find((x) => x.name === c.f);
  const field = f ? fieldTitle(f) : c.f;
  const val = (k: string) => (f ? groupLabel(f, k, lk) : k);
  switch (c.op) {
    case "contains":
      return t("{field} contains “{text}”", { field, text: c.v });
    case "is":
      return t("{field}: {values}", { field, values: fmt.list(c.v.map(val), "disjunction") });
    case "isnot":
      return t("{field} not {values}", { field, values: fmt.list(c.v.map(val), "disjunction") });
    case "bool":
      return t("{field}: {values}", { field, values: c.v ? t("Yes") : t("No") });
    case "month":
      return t("{field}: {values}", { field, values: monthLabel(c.v) });
    case "empty":
      return t("{field} is empty", { field });
    case "range": {
      const show = (s: string) => boundText(f, s);
      if (c.min && c.max)
        return t("{field}: {from} to {to}", { field, from: show(c.min), to: show(c.max) });
      if (c.min) return t("{field} ≥ {from}", { field, from: show(c.min) });
      return t("{field} ≤ {to}", { field, to: show(c.max ?? "") });
    }
  }
}

/** A stored range bound as the person reads it: a date in the locale, an amount with its currency. */
export function boundText(f: FieldMeta | undefined, s: string): string {
  if (f?.kind === "date" || f?.kind === "datetime") return formatDate(s);
  if (f?.kind === "decimal") {
    const d = rangeDecimal(s);
    return d === undefined ? s : decimalText(d, {});
  }
  const n = rangeNum(f ?? ({ kind: "number" } as FieldMeta), s);
  if (n === undefined) return s;
  return f?.kind === "money" ? money(n) : fmt.number(n, { maximumFractionDigits: 6 });
}

export function validRange(f: FieldMeta, c: Extract<Cond, { op: "range" }>): boolean {
  if (!c.min && !c.max) return false;
  if (f.kind === "date" || f.kind === "datetime")
    return (!c.min || validDate(c.min)) && (!c.max || validDate(c.max));
  if (f.kind === "decimal")
    return (
      (!c.min || rangeDecimal(c.min) !== undefined) && (!c.max || rangeDecimal(c.max) !== undefined)
    );
  return (
    (!c.min || rangeNum(f, c.min) !== undefined) && (!c.max || rangeNum(f, c.max) !== undefined)
  );
}

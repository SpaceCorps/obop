import type { FieldMeta, Kind, Row } from "./api.ts";
import {
  formatDate,
  integer,
  isEmpty,
  label,
  money,
  monthLabel,
  optionLabel,
  plainText,
  validDate,
} from "./format.ts";

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
    if (f.kind === "ref" && f.to)
      return lk.refText(f.to, a).localeCompare(lk.refText(f.to, b), undefined, { numeric: true });
    return a.localeCompare(b);
  });
}

export function groupLabel(f: FieldMeta, key: string, lk: Lookup): string {
  if (key === "") return "None";
  switch (f.kind) {
    case "select":
      return optionLabel(key);
    case "bool":
      return key === "true" ? "Yes" : "No";
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
  const present = rows.map((r) => groupKey(f, r));
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

export const sumOf = (rows: Row[], field: string) =>
  rows.reduce((n, r) => n + (Number(r[field]) || 0), 0);

export function formatSum(f: FieldMeta | undefined, n: number): string {
  return f?.kind === "money" ? money(n) : integer(n);
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

export function rangeNum(f: FieldMeta, s: string | undefined): number | undefined {
  if (s === undefined || s.trim() === "") return undefined;
  const n = parseFloat(s.replace(",", "."));
  if (!Number.isFinite(n)) return undefined;
  return f.kind === "money" ? Math.round(n * 100) : n;
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
    case "bool":
      return Number(Boolean(x)) - Number(Boolean(y));
    case "select": {
      const o = f.options ?? [];
      return o.indexOf(String(x)) - o.indexOf(String(y));
    }
    default:
      return plainText(f, x, lk.refText).localeCompare(plainText(f, y, lk.refText), undefined, {
        numeric: true,
        sensitivity: "base",
      });
  }
}

export function sortRows(rows: Row[], sort: Sort | undefined, fields: FieldMeta[], lk: Lookup) {
  const f = sort && fields.find((x) => x.name === sort.f);
  if (!sort || !f) return rows;
  const sign = sort.dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const ea = isEmpty(a[f.name]);
    const eb = isEmpty(b[f.name]);
    if (ea || eb) return Number(ea) - Number(eb); // empty values always last
    return sign * compare(f, a, b, lk);
  });
}

/** Chip text for a facet: "Status is Open", "Amount 100 to 500". */
export function describeCond(c: Cond, fields: FieldMeta[], lk: Lookup): string {
  if (c.op === "search") return `Search: ${c.v}`;
  const f = fields.find((x) => x.name === c.f);
  const name = f ? label(f.name) : c.f;
  const val = (k: string) => (f ? groupLabel(f, k, lk) : k);
  switch (c.op) {
    case "contains":
      return `${name} contains “${c.v}”`;
    case "is":
      return `${name}: ${c.v.map(val).join(" or ")}`;
    case "isnot":
      return `${name} not ${c.v.map(val).join(" or ")}`;
    case "bool":
      return `${name}: ${c.v ? "Yes" : "No"}`;
    case "month":
      return `${name}: ${monthLabel(c.v)}`;
    case "empty":
      return `${name} is empty`;
    case "range": {
      const fmt = (s: string) => (f?.kind === "date" ? formatDate(s) : s);
      if (c.min && c.max) return `${name}: ${fmt(c.min)} to ${fmt(c.max)}`;
      if (c.min) return `${name} ≥ ${fmt(c.min)}`;
      return `${name} ≤ ${fmt(c.max ?? "")}`;
    }
  }
}

export function validRange(f: FieldMeta, c: Extract<Cond, { op: "range" }>): boolean {
  if (!c.min && !c.max) return false;
  if (f.kind === "date") return (!c.min || validDate(c.min)) && (!c.max || validDate(c.max));
  return (
    (!c.min || rangeNum(f, c.min) !== undefined) && (!c.max || rangeNum(f, c.max) !== undefined)
  );
}

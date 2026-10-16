import {
  LOAD_LIMIT,
  search,
  type FieldMeta,
  type ModelMeta,
  type Row,
  type ServerFilter,
} from "../api.ts";
import { isEmpty } from "../format.ts";
import { groupDomain, groupKey, groupLabel, groupable, measurable, type Lookup } from "../query.ts";
import { parseFilter, toServerFilter, type Condition } from "./filter.ts";

export type WidgetKind = "kpi" | "bar" | "line" | "pie" | "table";
export const KINDS: WidgetKind[] = ["kpi", "bar", "line", "pie", "table"];

/** A `dashboards_widget` record. */
export type Widget = {
  id: string;
  dashboard_id: string;
  title: string;
  kind: WidgetKind;
  model_name: string;
  measure_field?: string | null;
  aggregate: "count" | "sum";
  group_by?: string | null;
  filter?: string | null;
  position?: number | null;
};

export type Dashboard = {
  id: string;
  title: string;
  owner: string;
  description?: string | null;
  shared?: boolean | null;
  position?: number | null;
};

/** What a widget reads, checked against the models this person can see. */
export type Resolved =
  | { ok: false; reason: "noaccess"; detail: string }
  | { ok: false; reason: "invalid"; detail: string }
  | {
      ok: true;
      model: ModelMeta;
      conds: Condition[];
      filter: ServerFilter[];
      measure?: FieldMeta;
      dim?: FieldMeta;
    };

/**
 * Read a widget against `/api/meta`. A model or field the viewer cannot see (or that does not
 * exist) gives `noaccess` before any request is made, so nothing about it leaks.
 */
export function resolve(w: Widget, models: Map<string, { model: ModelMeta }>): Resolved {
  const model = models.get(w.model_name)?.model;
  if (!model) return { ok: false, reason: "noaccess", detail: w.model_name };
  const parsed = parseFilter(w.filter ?? "");
  if ("error" in parsed) return { ok: false, reason: "invalid", detail: parsed.error };
  const sf = toServerFilter(parsed.conds, model);
  if ("noAccess" in sf) return { ok: false, reason: "noaccess", detail: sf.noAccess };
  if ("error" in sf) return { ok: false, reason: "invalid", detail: sf.error };
  let measure: FieldMeta | undefined;
  if (w.aggregate === "sum") {
    if (isEmpty(w.measure_field))
      return { ok: false, reason: "invalid", detail: "A sum needs a measure field" };
    measure = model.fields.find((f) => f.name === w.measure_field);
    if (!measure) return { ok: false, reason: "noaccess", detail: String(w.measure_field) };
    if (!measurable([measure]).length)
      return {
        ok: false,
        reason: "invalid",
        detail: `${measure.name} is not a number or money field`,
      };
  }
  let dim: FieldMeta | undefined;
  if (!isEmpty(w.group_by)) {
    dim = model.fields.find((f) => f.name === w.group_by);
    if (!dim) return { ok: false, reason: "noaccess", detail: String(w.group_by) };
    if (!groupable([dim]).length)
      return { ok: false, reason: "invalid", detail: `${dim.name} cannot be grouped by` };
  } else if (w.kind !== "kpi" && w.kind !== "table")
    return { ok: false, reason: "invalid", detail: "This chart needs a field to group by" };
  return { ok: true, model, conds: parsed.conds, filter: sf.filter, measure, dim };
}

export type Point = { key: string; label: string; value: number };

export type WidgetData =
  | { kind: "kpi"; value: number; measure?: FieldMeta; total: number; truncated: boolean }
  | {
      kind: "series";
      /** The records of the search; the card groups them, so ref labels can arrive later. */
      rows: Row[];
      measure?: FieldMeta;
      dim: FieldMeta;
      total: number;
      truncated: boolean;
    }
  | { kind: "rows"; rows: Row[]; model: ModelMeta; total: number };

export type Fetched =
  | { state: "ready"; data: WidgetData }
  | { state: "noaccess"; detail: string }
  | { state: "error"; message: string };

const sum = (rows: Row[], f: FieldMeta) => rows.reduce((n, r) => n + (Number(r[f.name]) || 0), 0);

/** The value of a group: its record count, or the sum of the measure. */
export const valueOf = (rows: Row[], measure?: FieldMeta) =>
  measure ? sum(rows, measure) : rows.length;

export function buildPoints(
  rows: Row[],
  dim: FieldMeta,
  measure: FieldMeta | undefined,
  lk: Lookup,
): Point[] {
  return groupDomain(dim, rows, lk).map((key) => ({
    key,
    label: groupLabel(dim, key, lk),
    value: valueOf(
      rows.filter((r) => groupKey(dim, r) === key),
      measure,
    ),
  }));
}

/** Ask the server for what a widget needs and shape it. Never throws. */
export async function fetchWidget(
  token: string,
  w: Widget,
  r: Extract<Resolved, { ok: true }>,
): Promise<Fetched> {
  const { model, filter, measure, dim } = r;
  const q = (limit: number, extra: object = {}) =>
    search(token, model.name, { filter: filter.length ? filter : undefined, limit, ...extra });
  const fail = (e: { error: string }): Fetched =>
    /unknown field /.test(e.error)
      ? { state: "noaccess", detail: e.error }
      : { state: "error", message: e.error };
  // A field withheld from this person is not worth a chart: show no access, not zeros.
  const withheld = (denied: string[]) =>
    [measure, dim].find((f) => f && denied.includes(`${model.name}.${f.name}`));

  if (w.kind === "kpi" && !measure) {
    const res = await q(1);
    if ("error" in res) return fail(res);
    return {
      state: "ready",
      data: { kind: "kpi", value: res.total, total: res.total, truncated: false },
    };
  }
  if (w.kind === "table" && !dim) {
    const res = await q(8);
    if ("error" in res) return fail(res);
    return { state: "ready", data: { kind: "rows", rows: res.rows, model, total: res.total } };
  }
  const res = await q(LOAD_LIMIT);
  if ("error" in res) return fail(res);
  const hidden = withheld(res.denied);
  if (hidden) return { state: "noaccess", detail: hidden.name };
  const truncated = res.total > res.rows.length;
  if (w.kind === "kpi")
    return {
      state: "ready",
      data: { kind: "kpi", value: sum(res.rows, measure!), measure, total: res.total, truncated },
    };
  return {
    state: "ready",
    data: {
      kind: "series",
      rows: res.rows,
      measure,
      dim: dim!,
      total: res.total,
      truncated,
    },
  };
}

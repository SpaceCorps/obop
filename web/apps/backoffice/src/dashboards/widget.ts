import { t } from "@obop/ui";
import {
  aggregate,
  search,
  type FieldMeta,
  type ModelMeta,
  type Row,
  type ServerFilter,
} from "../api.ts";
import { isEmpty } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { domainOf, groupLabel, groupable, measurable, type Lookup } from "../query.ts";
import { groupByFor, keyOf, measuresFor, valueOfGroup } from "../useAggregate.ts";
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
      return { ok: false, reason: "invalid", detail: t("A sum needs a measure field") };
    measure = model.fields.find((f) => f.name === w.measure_field);
    if (!measure) return { ok: false, reason: "noaccess", detail: String(w.measure_field) };
    if (!measurable([measure]).length)
      return {
        ok: false,
        reason: "invalid",
        detail: t("{field} is not a number or money field", { field: fieldTitle(measure) }),
      };
  }
  let dim: FieldMeta | undefined;
  if (!isEmpty(w.group_by)) {
    dim = model.fields.find((f) => f.name === w.group_by);
    if (!dim) return { ok: false, reason: "noaccess", detail: String(w.group_by) };
    if (!groupable([dim]).length)
      return {
        ok: false,
        reason: "invalid",
        detail: t("{field} cannot be grouped by", { field: fieldTitle(dim) }),
      };
  } else if (w.kind !== "kpi" && w.kind !== "table")
    return { ok: false, reason: "invalid", detail: t("This chart needs a field to group by") };
  return { ok: true, model, conds: parsed.conds, filter: sf.filter, measure, dim };
}

export type Point = { key: string; label: string; value: number };

export type WidgetData =
  | { kind: "kpi"; value: number; measure?: FieldMeta; total: number; truncated: boolean }
  | {
      kind: "series";
      /** The server's groups (D4): the value of each group key; the card labels them. */
      groups: { key: string; value: number }[];
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

/** The labelled points of a chart from the server's groups (labels of refs arrive later). */
export function buildPoints(
  groups: { key: string; value: number }[],
  dim: FieldMeta,
  lk: Lookup,
): Point[] {
  const value = new Map(groups.map((g) => [g.key, g.value]));
  return domainOf(dim, [...value.keys()], lk).map((key) => ({
    key,
    label: groupLabel(dim, key, lk),
    value: value.get(key) ?? 0,
  }));
}

/** Ask the server for what a widget needs and shape it. Never throws. */
export async function fetchWidget(
  token: string,
  w: Widget,
  r: Extract<Resolved, { ok: true }>,
): Promise<Fetched> {
  const { model, filter, measure, dim } = r;
  const fail = (e: { error: string }): Fetched =>
    /unknown field /.test(e.error)
      ? { state: "noaccess", detail: e.error }
      : { state: "error", message: e.error };

  if (w.kind === "table" && !dim) {
    const res = await search(token, model.name, {
      filter: filter.length ? filter : undefined,
      limit: 8,
    });
    if ("error" in res) return fail(res);
    return { state: "ready", data: { kind: "rows", rows: res.rows, model, total: res.total } };
  }
  // Counts and sums are computed by the server over every matching record (D4), under this
  // person's access rules: a field they cannot read answers "unknown field" and shows "No access".
  const res = await aggregate(token, model.name, {
    filter,
    group_by: dim ? [groupByFor(dim)] : [],
    measures: measuresFor(measure),
  });
  if ("error" in res) return fail(res);
  if (w.kind === "kpi")
    return {
      state: "ready",
      data: {
        kind: "kpi",
        value: measure ? (res.groups[0] ? valueOfGroup(res.groups[0], measure) : 0) : res.total,
        measure,
        total: res.total,
        truncated: false,
      },
    };
  return {
    state: "ready",
    data: {
      kind: "series",
      groups: res.groups.map((g) => ({
        key: keyOf(g.key[dim!.name]),
        value: valueOfGroup(g, measure),
      })),
      measure,
      dim: dim!,
      total: res.total,
      truncated: false,
    },
  };
}

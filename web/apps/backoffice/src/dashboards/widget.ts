import { t } from "@obop/ui";
import { search, type FieldMeta, type ModelMeta, type Row, type ServerFilter } from "../api.ts";
import { isEmpty } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import {
  BUCKETS,
  currencySplit,
  dimensionsOf,
  type Bucket,
  type Dim,
  type MeasureSel,
} from "../reports.ts";
import { fetchReport, type ReportAnswer } from "../useAggregate.ts";
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
  /** The period a date or date-and-time split counts per (dashboards 0.4.0); empty is month. */
  period?: Bucket | null;
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

/** The kinds of field a tile can add up. */
const SUMMABLE_KINDS = new Set<FieldMeta["kind"]>(["number", "money", "decimal"]);

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
      /** The period of a dated split, when the widget names one (K1). */
      period?: Bucket;
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
    if (!SUMMABLE_KINDS.has(measure.kind))
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
    if (!dimensionsOf([dim]).length)
      return {
        ok: false,
        reason: "invalid",
        detail: t("{field} cannot be grouped by", { field: fieldTitle(dim) }),
      };
  } else if (w.kind !== "kpi" && w.kind !== "table")
    return { ok: false, reason: "invalid", detail: t("This chart needs a field to group by") };
  const period = BUCKETS.find((b) => b === w.period);
  return { ok: true, model, conds: parsed.conds, filter: sf.filter, measure, dim, period };
}

/** What a resolved widget measures: the count, or the sum of its field. */
export const widgetMeasure = (r: Extract<Resolved, { ok: true }>): MeasureSel =>
  r.measure ? { field: r.measure, op: "sum" } : null;

/** What a resolved widget splits by: its field, a date or instant by its period (month unless set). */
export const widgetDim = (r: Extract<Resolved, { ok: true }>): Dim | undefined =>
  r.dim
    ? {
        field: r.dim,
        ...(r.dim.kind === "date" || r.dim.kind === "datetime"
          ? { bucket: r.period ?? ("month" as const) }
          : {}),
      }
    : undefined;

export type WidgetData =
  | {
      kind: "report";
      /** The server's groups (D4), with the value that tells each group's currency. */
      answer: ReportAnswer;
      measure: MeasureSel;
      dim?: Dim;
      /** The field the groups are split by for their currency, when the measure is money. */
      split?: string;
    }
  | { kind: "rows"; rows: Row[]; model: ModelMeta; total: number };

export type Fetched =
  | { state: "ready"; data: WidgetData }
  | { state: "noaccess"; detail: string }
  | { state: "error"; message: string };

/** Ask the server for what a widget needs and shape it. Never throws. */
export async function fetchWidget(
  token: string,
  w: Widget,
  r: Extract<Resolved, { ok: true }>,
): Promise<Fetched> {
  const { model, filter } = r;
  const fail = (e: { error: string }): Fetched =>
    /unknown field /.test(e.error)
      ? { state: "noaccess", detail: e.error }
      : { state: "error", message: e.error };

  const dim = widgetDim(r);
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
  const measure = widgetMeasure(r);
  const split = currencySplit(model, measure)?.field;
  const res = await fetchReport(token, {
    model: model.name,
    filter,
    dims: dim && w.kind !== "kpi" ? [dim] : [],
    measure,
    split,
  });
  if ("error" in res) return fail(res);
  return {
    state: "ready",
    data: { kind: "report", answer: res, measure, dim: w.kind === "kpi" ? undefined : dim, split },
  };
}

import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrency } from "@obop/ui";
import {
  aggregate,
  getTime,
  type AggGroup,
  type AggGroupBy,
  type AggMeasure,
  type FieldMeta,
  type ModelMeta,
  type Row,
  type ServerFilter,
} from "./api.ts";
import { useData } from "./data.tsx";
import { currencyOf as presentCurrency, titleOf, type PresentCtx } from "./presentation.ts";
import type { Lookup } from "./query.ts";
import {
  currencySplit,
  dimRef,
  groupByOf,
  isMoney,
  measureAggs,
  measureFilter,
  measureRef,
  reportFromGroups,
  reportFromRows,
  type Dim,
  type MeasureSel,
  type ReportRow,
} from "./reports.ts";

export { keyOf } from "./reports.ts";

export type Aggregated = {
  groups: AggGroup[];
  /** Records the filter matches. */
  total: number;
  loaded: boolean;
  loading: boolean;
  error?: string;
  status?: number;
  /** Ask again (after an error). */
  retry: () => void;
};

const idle: Omit<Aggregated, "retry"> = { groups: [], total: 0, loaded: false, loading: false };

/**
 * Groups and totals from `<model>.search` with `aggregate` (D4). It asks again when the query
 * changes and after any successful write, ignores answers that arrive late and keeps the previous
 * answer on screen while the next one loads.
 */
export function useAggregate(opts: {
  enabled: boolean;
  model: string;
  filter: ServerFilter[];
  groupBy: AggGroupBy[];
  measures: AggMeasure[];
}): Aggregated {
  const d = useData();
  const [st, setSt] = useState<Omit<Aggregated, "retry">>(idle);
  const [attempt, setAttempt] = useState(0);
  const seq = useRef(0);
  const { enabled, model, filter, groupBy, measures } = opts;
  const key = JSON.stringify([model, filter, groupBy, measures]);

  useEffect(() => {
    if (!enabled) return;
    const mine = ++seq.current;
    setSt((s) => ({ ...s, loading: true, error: undefined }));
    void aggregate(d.token, model, { filter, group_by: groupBy, measures }).then((r) => {
      if (mine !== seq.current) return;
      if ("error" in r)
        setSt((s) => ({ ...s, loading: false, loaded: true, error: r.error, status: r.status }));
      else setSt({ groups: r.groups, total: r.total, loaded: true, loading: false });
    });
    // The key stands for the model, filter, grouping and measures.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, d.token, key, d.mutations, attempt]);

  useEffect(() => {
    setSt(idle);
  }, [model]);

  return { ...st, retry: () => setAttempt((n) => n + 1) };
}

/** The group-by entry that yields the keys `groupKey()` produces (dates by month). */
export const groupByFor = (f: FieldMeta): AggGroupBy =>
  f.kind === "date" ? { field: f.name, bucket: "month" } : { field: f.name };

/** The measures of a view: the count and, when chosen, the sum of one number or money field. */
export const measuresFor = (m: FieldMeta | undefined): AggMeasure[] =>
  m ? [{ op: "count" }, { op: "sum", field: m.name }] : [{ op: "count" }];

/** The summed value of a group: the sum of the measure, or the record count. */
export const valueOfGroup = (g: AggGroup, m: FieldMeta | undefined): number =>
  m ? Number(g.measures[`sum:${m.name}`] ?? 0) || 0 : g.count;

// ---- reports: what the pivot, the graph, the dashboard tiles and the overviews read -------------

/** What a report asks: the records `filter` matches, grouped by `dims`, adding up `measure`. */
export type ReportQuery = {
  model: string;
  filter: ServerFilter[];
  dims: readonly Dim[];
  measure: MeasureSel;
  /** The field whose value tells a money group's currency (`currencySplit`); none otherwise. */
  split?: string;
};

/** A server group with the value of the split field beside its key. */
export type ReportGroup = AggGroup & { split?: unknown };

export type ReportAnswer = {
  groups: ReportGroup[];
  total: number;
  /** The values of the split field among the records. */
  splitValues: unknown[];
  /** True when the groups could not be split (too many values to ask for one by one). */
  unsplit: boolean;
};

/** The most values of the split field asked for one at a time (two dimensions and money). */
const MAX_SPLIT_QUERIES = 12;

/**
 * Ask the server for a report (D4). Money is grouped by its currency as well: with one dimension
 * the split field is a second group-by; with two (the server's limit) the report is asked once per
 * value of the split field. Never throws.
 */
export async function fetchReport(
  token: string,
  q: ReportQuery,
): Promise<ReportAnswer | { error: string; status: number }> {
  const filter = [...q.filter, ...measureFilter(q.measure)];
  const measures = measureAggs(q.measure);
  const by = q.dims.map(groupByOf);
  const split = q.split;
  if (!split) {
    const r = await aggregate(token, q.model, { filter, group_by: by, measures });
    if ("error" in r) return r;
    return { groups: r.groups, total: r.total, splitValues: [], unsplit: false };
  }
  if (by.length < 2) {
    const r = await aggregate(token, q.model, {
      filter,
      group_by: [...by, { field: split }],
      measures,
    });
    if ("error" in r) return r;
    const groups = r.groups.map((g) => {
      const { [split]: value, ...key } = g.key;
      return { ...g, key, split: value ?? null };
    });
    return {
      groups,
      total: r.total,
      splitValues: [...new Set(groups.map((g) => g.split))],
      unsplit: false,
    };
  }
  const values = await aggregate(token, q.model, {
    filter,
    group_by: [{ field: split }],
    measures: [{ op: "count" }],
  });
  if ("error" in values) return values;
  const splitValues = values.groups.map((g) => g.key[split] ?? null);
  if (splitValues.length <= 1 || splitValues.length > MAX_SPLIT_QUERIES) {
    const r = await aggregate(token, q.model, { filter, group_by: by, measures });
    if ("error" in r) return r;
    const one = splitValues.length === 1 ? splitValues[0] : undefined;
    return {
      groups: r.groups.map((g) => ({ ...g, split: one })),
      total: r.total,
      splitValues,
      unsplit: splitValues.length > 1,
    };
  }
  const parts = await Promise.all(
    splitValues.map((v) =>
      aggregate(token, q.model, {
        filter: [
          ...filter,
          v === null
            ? { field: split, op: "is_null", value: true }
            : { field: split, op: "eq", value: v },
        ],
        group_by: by,
        measures,
      }),
    ),
  );
  const groups: ReportGroup[] = [];
  let total = 0;
  for (const [i, r] of parts.entries()) {
    if ("error" in r) return r;
    total += r.total;
    for (const g of r.groups) groups.push({ ...g, split: splitValues[i] });
  }
  return { groups, total, splitValues, unsplit: false };
}

type ReportFetch = ReportAnswer & {
  loaded: boolean;
  loading: boolean;
  error?: string;
  status?: number;
};
const idleReport: ReportFetch = {
  groups: [],
  total: 0,
  splitValues: [],
  unsplit: false,
  loaded: false,
  loading: false,
};

/** `fetchReport` as a hook: asks again when the query changes and after any successful write. */
export function useReport(q: ReportQuery & { enabled: boolean }): ReportFetch & {
  retry: () => void;
} {
  const d = useData();
  const [st, setSt] = useState<ReportFetch>(idleReport);
  const [attempt, setAttempt] = useState(0);
  const seq = useRef(0);
  const key = JSON.stringify([
    q.model,
    q.filter,
    q.dims.map(dimRef),
    measureRef(q.measure),
    q.split ?? "",
  ]);
  useEffect(() => {
    if (!q.enabled) return;
    const mine = ++seq.current;
    setSt((s) => ({ ...s, loading: true, error: undefined }));
    void fetchReport(d.token, q).then((r) => {
      if (mine !== seq.current) return;
      if ("error" in r)
        setSt((s) => ({ ...s, loading: false, loaded: true, error: r.error, status: r.status }));
      else setSt({ ...r, loaded: true, loading: false });
    });
    // The key stands for the query; `mutations` asks again after any write.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q.enabled, d.token, key, d.mutations, attempt]);
  useEffect(() => {
    setSt(idleReport);
  }, [q.model]);
  return { ...st, retry: () => setAttempt((n) => n + 1) };
}

/** What a presentation function needs to follow `ui.currency_from` through cached rows. */
export function usePresentCtx(): PresentCtx {
  const d = useData();
  return {
    instanceCurrency: getCurrency(),
    rowOf: (model, id) => d.store.peek(model)?.find((r) => r.id === id),
    modelOf: (model) => d.models.get(model)?.model,
    refText: d.refText,
  };
}

/**
 * The look-ups of the report views with records named by `titleOf` (an account reads "1400 Trade
 * receivables", not "1400"), falling back to the data layer's label for a record not loaded.
 */
export function useTitleLookup(): Lookup {
  const d = useData();
  const ctx = usePresentCtx();
  return useMemo(() => {
    const refText = (model: string, id: string) => {
      const info = d.models.get(model);
      const row = d.store.peek(model)?.find((r) => r.id === id);
      return info && row
        ? titleOf(info.model, row, { ...ctx, moduleId: info.module.id })
        : d.refText(model, id);
    };
    return { refText, refIds: d.lookup.refIds };
    // `d` is a new object when rows load or the language changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d, d.lang]);
}

/** A report ready to draw, with its state; what the views and tiles read. */
export type ReportState = {
  rows: ReportRow[];
  /** Records the filter matches. */
  total: number;
  loaded: boolean;
  loading: boolean;
  error?: string;
  status?: number;
  retry: () => void;
  /** Money in more than one currency that could not be told apart: the sums mix them. */
  mixed: boolean;
};

/**
 * A report over a model: asked of the server when `server` is set (D4), else added up from `rows`
 * in the browser. Money comes back one row per currency, resolved from the record's `currency`
 * field or from `ui.currency_from` through cached rows (the instance's currency otherwise).
 */
export function useReportRows(opts: {
  server: boolean;
  enabled?: boolean;
  model: ModelMeta;
  filter: ServerFilter[];
  dims: readonly Dim[];
  measure: MeasureSel;
  rows?: readonly Row[];
}): ReportState {
  const d = useData();
  const ctx = usePresentCtx();
  const { model, measure, dims } = opts;
  const split = currencySplit(model, measure);
  const enabled = opts.enabled !== false;
  const got = useReport({
    enabled: enabled && opts.server,
    model: model.name,
    filter: opts.filter,
    dims,
    measure,
    split: split?.field,
  });
  const toCurrency = (value: unknown): string | undefined => {
    if (!split) return ctx.instanceCurrency;
    const row = { id: "", [split.field]: value } as Row;
    return presentCurrency(row, model, ctx, measure?.field);
  };
  const dimKey = dims.map(dimRef).join();
  const rows = useMemo(
    () =>
      opts.server
        ? reportFromGroups(got.groups, dims, measure, toCurrency)
        : reportFromRows(opts.rows ?? [], dims, measure, {
            currencyOf: (r) => presentCurrency(r, model, ctx, measure?.field),
            zone: getTime().zone,
          }),
    // `version` brings the rows a `currency_from` path reads; the keys stand for the query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [opts.server, got.groups, opts.rows, dimKey, measureRef(measure), model, d.version],
  );
  if (!opts.server)
    return {
      rows,
      total: opts.rows?.length ?? 0,
      loaded: true,
      loading: false,
      retry: got.retry,
      mixed: false,
    };
  const mixed =
    isMoney(measure) && got.unsplit && new Set(got.splitValues.map(toCurrency)).size > 1;
  return {
    rows,
    total: got.total,
    loaded: got.loaded,
    loading: got.loading,
    error: got.error,
    status: got.status,
    retry: got.retry,
    mixed,
  };
}

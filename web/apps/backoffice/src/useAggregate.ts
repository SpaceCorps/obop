import { useEffect, useRef, useState } from "react";
import {
  aggregate,
  type AggGroup,
  type AggGroupBy,
  type AggMeasure,
  type FieldMeta,
  type ModelMeta,
  type ServerFilter,
} from "./api.ts";
import { useData } from "./data.tsx";

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

/** The string key the views use for a group value: "" for empty, "true"/"false" for a bool. */
export function keyOf(v: unknown): string {
  if (v === null || v === undefined || v === "") return "";
  if (typeof v === "boolean") return v ? "true" : "false";
  return String(v);
}

/** The group-by entry that yields the keys `groupKey()` produces (dates by month). */
export const groupByFor = (f: FieldMeta): AggGroupBy =>
  f.kind === "date" ? { field: f.name, bucket: "month" } : { field: f.name };

/** The measures of a view: the count and, when chosen, the sum of one number or money field. */
export const measuresFor = (m: FieldMeta | undefined): AggMeasure[] =>
  m ? [{ op: "count" }, { op: "sum", field: m.name }] : [{ op: "count" }];

/** The summed value of a group: the sum of the measure, or the record count. */
export const valueOfGroup = (g: AggGroup, m: FieldMeta | undefined): number =>
  m ? (g.measures[`sum:${m.name}`] ?? 0) : g.count;

/**
 * The one currency of the records a view adds up, and whether there are several (the sums would
 * then mix them). Only models with a `currency` field have anything to ask; one aggregate says.
 */
export function useCurrencyOf(
  enabled: boolean,
  model: ModelMeta,
  filter: ServerFilter[],
): { currency?: string; mixed: boolean } {
  const has = model.fields.some(
    (f) => f.name === "currency" && (f.kind === "text" || f.kind === "select"),
  );
  const a = useAggregate({
    enabled: enabled && has,
    model: model.name,
    filter,
    groupBy: [{ field: "currency" }],
    measures: [{ op: "count" }],
  });
  const set = new Set(
    a.groups
      .map((g) => g.key.currency)
      .filter((c): c is string => typeof c === "string" && /^[A-Z]{3}$/.test(c)),
  );
  return { currency: set.size === 1 ? [...set][0] : undefined, mixed: set.size > 1 };
}

import { useEffect, useRef, useState } from "react";
import { call, search, type Row, type ServerFilter, type ServerOrder } from "./api.ts";
import { useData } from "./data.tsx";

export type ServerRows = {
  rows: Row[];
  /** Matches before paging. */
  total: number;
  denied: string[];
  /** The first answer has arrived (later pages keep showing the previous rows meanwhile). */
  loaded: boolean;
  loading: boolean;
  error?: string;
  status?: number;
};

const idle: ServerRows = { rows: [], total: 0, denied: [], loaded: false, loading: false };

/**
 * One page of a model from `<model>.search`. It asks again when the query changes and after any
 * successful write, ignores answers that arrive late, and keeps the previous page on screen while
 * the next one loads so the table does not flash.
 */
export function useServerRows(opts: {
  enabled: boolean;
  model: string;
  filter: ServerFilter[];
  order: ServerOrder[];
  offset: number;
  limit: number;
}): ServerRows {
  const d = useData();
  const [st, setSt] = useState<ServerRows>(idle);
  const seq = useRef(0);
  const { enabled, model, filter, order, offset, limit } = opts;
  const key = JSON.stringify([model, filter, order, offset, limit]);

  useEffect(() => {
    if (!enabled) return;
    const mine = ++seq.current;
    setSt((s) => ({ ...s, loading: true, error: undefined }));
    const q: Record<string, unknown> = { limit, offset };
    if (filter.length) q.filter = filter;
    if (order.length) q.order = order;
    void search(d.token, model, q).then((r) => {
      if (mine !== seq.current) return;
      if ("error" in r)
        setSt((s) => ({ ...s, loading: false, loaded: true, error: r.error, status: r.status }));
      else
        setSt({
          rows: r.rows,
          total: r.total,
          denied: r.denied,
          loaded: true,
          loading: false,
        });
    });
    // The key stands for model, filter, order, offset and limit.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, d.token, key, d.mutations]);

  // A model change starts from scratch.
  useEffect(() => {
    setSt(idle);
  }, [model]);

  return st;
}

/** One record by id: from the rows already on screen when possible, else fetched with `get`. */
export function useRecord(model: string, id: string, onScreen: Row[]): Row | null | undefined {
  const d = useData();
  const have = id && id !== "new" ? onScreen.find((r) => r.id === id) : undefined;
  const [fetched, setFetched] = useState<{ id: string; row: Row | null } | null>(null);
  const need = !!id && id !== "new" && !have;
  useEffect(() => {
    if (!need) return;
    let live = true;
    void call(d.token, `${model}.get`, { id }).then((r) => {
      if (live) setFetched({ id, row: r.kind === "ok" ? (r.value as Row) : null });
    });
    return () => {
      live = false;
    };
  }, [need, d.token, model, id, d.mutations]);
  if (!id || id === "new") return undefined;
  if (have) return have;
  return fetched?.id === id ? fetched.row : undefined;
}

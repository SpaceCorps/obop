import { useEffect, useRef, useState } from "react";
import { useI18n } from "@obop/ui";
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
 * One page of a model from `<model>.search`. It asks again when the query changes, after any
 * successful write and, for a sorted query, when the locale changes (the server sorts text in the
 * collation of the locale it is told, and `search` tells it only when there is a sort). It ignores
 * answers that arrive late, and keeps the previous page on screen while the next one loads so the
 * table does not flash.
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
  const { locale } = useI18n();
  const [st, setSt] = useState<ServerRows>(idle);
  const seq = useRef(0);
  const { enabled, model, filter, order, offset, limit } = opts;
  // The locale is part of the query only where the request carries it (`lang`, api.ts): a sort.
  const key = JSON.stringify([model, filter, order, offset, limit, order.length ? locale : null]);

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
    // The key stands for model, filter, order, offset, limit and, for a sort, the locale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, d.token, key, d.mutations]);

  // A model change starts from scratch.
  useEffect(() => {
    setSt(idle);
  }, [model]);

  return st;
}

/**
 * One record by id: from the rows already on screen when possible, else fetched with `get`. While
 * the rows are being asked for again (a new sort, the language) and the record is no longer among
 * them, the last row seen stays until the `get` answers: the page unmounts an open record panel
 * the moment this returns nothing, and a draft in it would be lost.
 */
export function useRecord(model: string, id: string, onScreen: Row[]): Row | null | undefined {
  const d = useData();
  const have = id && id !== "new" ? onScreen.find((r) => r.id === id) : undefined;
  const [fetched, setFetched] = useState<{ id: string; row: Row | null } | null>(null);
  const last = useRef<{ model: string; id: string; row: Row } | null>(null);
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
  const row =
    !id || id === "new" ? undefined : (have ?? (fetched?.id === id ? fetched.row : undefined));
  useEffect(() => {
    if (row) last.current = { model, id, row };
  }, [row, model, id]);
  if (!id || id === "new") return undefined;
  if (row !== undefined) return row;
  return last.current?.model === model && last.current.id === id ? last.current.row : undefined;
}

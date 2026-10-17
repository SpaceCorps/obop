import { NETWORK_MESSAGE } from "./net.ts";
import type { Reply, Row } from "./api.ts";

/**
 * Bulk work over "all matching" records (D1). The browser asks the server for the matching records
 * in pages, then applies the operation in small parallel batches so a long run neither floods the
 * server nor loses track: it reports progress after each batch, can be cancelled between batches,
 * stops when the connection is gone, and ends with one result per record.
 */

/** The most records one set-field or delete operation touches. */
export const BULK_CAP = 5000;
/** The most records one CSV export streams. */
export const EXPORT_CAP = 20000;
/** Records fetched per request while collecting. */
export const PAGE = 1000;
/** Records changed at the same time. */
export const BATCH = 8;

export type Outcome = {
  id: string;
  label: string;
  /** done, waiting for a human (agents), refused because other records use it, failed, or never reached. */
  status: "done" | "approval" | "referenced" | "failed" | "skipped";
  message?: string;
  planId?: string;
};

/** The outcome of one command reply. */
export function outcomeOf(row: { id: string; label: string }, r: Reply): Outcome {
  if (r.kind === "ok") return { ...row, status: "done" };
  if (r.kind === "approval") return { ...row, status: "approval", planId: r.planId };
  return {
    ...row,
    status: /still referenced by /.test(r.message) ? "referenced" : "failed",
    message: r.message,
  };
}

export type BatchRun = {
  results: Outcome[];
  /** The person cancelled; the records after the last finished batch were not touched. */
  cancelled: boolean;
  /** The connection dropped; the rest were not tried. */
  stopped: boolean;
};

/** Apply `work` to every item, `batch` at a time, in order. */
export async function inBatches<T extends { id: string; label: string }>(
  items: T[],
  work: (item: T) => Promise<Outcome>,
  o: {
    batch?: number;
    cancelled?: () => boolean;
    onProgress?: (done: number, total: number) => void;
  } = {},
): Promise<BatchRun> {
  const size = Math.max(1, o.batch ?? BATCH);
  const results: Outcome[] = [];
  let cancelled = false;
  let stopped = false;
  for (let i = 0; i < items.length; i += size) {
    if (o.cancelled?.()) {
      cancelled = true;
      break;
    }
    const chunk = items.slice(i, i + size);
    const out = await Promise.all(
      chunk.map((item) =>
        work(item).catch((): Outcome => ({
          id: item.id,
          label: item.label,
          status: "failed",
          message: NETWORK_MESSAGE,
        })),
      ),
    );
    results.push(...out);
    o.onProgress?.(results.length, items.length);
    if (out.every((r) => r.status === "failed" && r.message === NETWORK_MESSAGE)) {
      stopped = true;
      break;
    }
  }
  return { results, cancelled, stopped };
}

export type Collected =
  | { rows: Row[]; total: number; truncated: boolean; cancelled: boolean }
  | { error: string };

/**
 * Collect the records of a search page by page, up to `cap`. `page` fetches one page; `total` of the
 * first answer decides how many pages there are.
 */
export async function collect(
  page: (
    offset: number,
    limit: number,
  ) => Promise<{ rows: Row[]; total: number } | { error: string }>,
  cap: number,
  o: {
    pageSize?: number;
    cancelled?: () => boolean;
    onProgress?: (done: number, total: number) => void;
  } = {},
): Promise<Collected> {
  const size = o.pageSize ?? PAGE;
  const rows: Row[] = [];
  let total = Infinity;
  while (rows.length < Math.min(total, cap)) {
    if (o.cancelled?.())
      return {
        rows,
        total: Number.isFinite(total) ? total : rows.length,
        truncated: false,
        cancelled: true,
      };
    const want = Math.min(size, cap - rows.length);
    const r = await page(rows.length, want);
    if ("error" in r) return r;
    total = r.total;
    rows.push(...r.rows);
    o.onProgress?.(rows.length, Math.min(total, cap));
    if (r.rows.length === 0) break;
  }
  return { rows, total, truncated: total > rows.length, cancelled: false };
}

/** Counts per status, for the summary sentence. */
export function tally(results: Outcome[]): Record<Outcome["status"], number> {
  const n = { done: 0, approval: 0, referenced: 0, failed: 0, skipped: 0 };
  for (const r of results) n[r.status]++;
  return n;
}

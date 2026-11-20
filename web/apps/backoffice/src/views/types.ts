import type { Wording } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row, ServerFilter, ServerOrder } from "../api.ts";
import type { Collected } from "../bulk.ts";
import type { Cond, ViewState } from "../query.ts";

/** What every view gets from the model page. */
export type ViewProps = {
  model: ModelMeta;
  title: string;
  fields: FieldMeta[];
  /** The filtered, sorted rows (empty for a view that reads from the server, see `server`). */
  rows: Row[];
  /**
   * Present when the whole search is a server filter (D4): kanban, pivot, graph and calendar then
   * ask the server for counts, sums and pages instead of loading the records. Absent, `rows` holds
   * the records (the browser fallback for searches the server cannot express).
   */
  server?: { filter: ServerFilter[]; order: ServerOrder[] };
  state: ViewState;
  setState: (s: ViewState) => void;
  selectedId: string;
  onOpen: (id: string) => void;
  canCreate: boolean;
  canUpdate: boolean;
  /**
   * Returns the words of what went wrong, or null when it worked. They are worded when they are
   * drawn, so a message a view keeps follows a language switch (D9).
   */
  update: (id: string, patch: Record<string, unknown>) => Promise<Wording | null>;
  create: (body: Record<string, unknown>) => Promise<Wording | null>;
  /** Move a record along a governed field through its action; returns the words, or null when it worked. */
  moveState: (id: string, field: string, to: string) => Promise<Wording | null>;
  /** The server's total for the current search (the length of `rows` when the page is everything). */
  total: number;
  /** Paging of the list: present only where the list pages. */
  pager?: {
    offset: number;
    limit: number;
    busy: boolean;
    setOffset: (o: number) => void;
    setLimit: (n: number) => void;
  };
  /**
   * Every record the search matches, up to `cap`, fetched in pages (D1): for CSV export and for bulk
   * actions on "all matching". Reports progress and stops when `cancelled()` says so.
   */
  fetchAll: (o: {
    cap: number;
    cancelled?: () => boolean;
    onProgress?: (done: number, total: number) => void;
  }) => Promise<Collected>;
  canDelete: boolean;
  /** Show a dialog above the page (kept when the view is replaced), or hide it with `null`. */
  setDialog: (node: import("react").ReactNode) => void;
  /** Open the full form with some fields filled in. */
  newWith: (defaults: Record<string, unknown>) => void;
  /** Open the list narrowed to these conditions. */
  drill: (conds: Cond[]) => void;
};

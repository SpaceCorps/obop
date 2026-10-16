import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import type { Cond, ViewState } from "../query.ts";

/** What every view gets from the model page. */
export type ViewProps = {
  model: ModelMeta;
  title: string;
  fields: FieldMeta[];
  /** The filtered, sorted rows. */
  rows: Row[];
  state: ViewState;
  setState: (s: ViewState) => void;
  selectedId: string;
  onOpen: (id: string) => void;
  canCreate: boolean;
  canUpdate: boolean;
  /** Returns an error message, or null when it worked. */
  update: (id: string, patch: Record<string, unknown>) => Promise<string | null>;
  create: (body: Record<string, unknown>) => Promise<string | null>;
  /** Move a record along a governed field through its action; returns a message, or null when it worked. */
  moveState: (id: string, field: string, to: string) => Promise<string | null>;
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
  /** Every record the search matches (up to 5,000), for export and "select all". */
  exportAll: () => Promise<{ rows: Row[]; truncated: boolean }>;
  canDelete: boolean;
  /** Open the full form with some fields filled in. */
  newWith: (defaults: Record<string, unknown>) => void;
  /** Open the list narrowed to these conditions. */
  drill: (conds: Cond[]) => void;
};

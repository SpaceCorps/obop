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
  /** Open the full form with some fields filled in. */
  newWith: (defaults: Record<string, unknown>) => void;
  /** Open the list narrowed to these conditions. */
  drill: (conds: Cond[]) => void;
};

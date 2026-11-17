// `ui.readonly_when` of a field (R6): a condition of the display subset (displayCondition.ts) on the
// saved record while which the form shows the field read-only, so a person sees the rule the server
// holds before a save is refused. It is a hint: the server answers as it always did, and a new
// record is never held by it (the condition reads what was saved, not what is being typed).
import type { FieldMeta, Row } from "./api.ts";
import { compileCondition, type Condition } from "./displayCondition.ts";

// Compiled once per field object of the metadata; `null`: a condition this browser cannot read,
// which leaves the field as it would be without the hint (the server still refuses a wrong save).
const compiled = new WeakMap<FieldMeta, Condition | null>();

/** Whether the form shows `f` read-only on the saved record `row` by its `readonly_when`. */
export function readonlyByState(f: FieldMeta, row: Row | null): boolean {
  const src = f.ui?.readonly_when;
  if (!src || !row) return false;
  let c = compiled.get(f);
  if (c === undefined) {
    c = compileCondition(src);
    compiled.set(f, c);
  }
  return !!c && c(row);
}

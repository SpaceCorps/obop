// `ui.readonly_when` of a field (R6): a condition of the display subset (displayCondition.ts) on the
// saved record while which the form shows the field read-only, so a person sees the rule the server
// holds before a save is refused. It may read `$user`, the login of the person viewing the record
// (M2: the fields of a trip are its driver's). It is a hint: the server answers as it always did,
// and a new record is never held by it (the condition reads what was saved, not what is typed).
import type { FieldMeta, Row } from "./api.ts";
import { compileCondition, type Condition } from "./displayCondition.ts";

// Compiled once per field object of the metadata; `null`: a condition this browser cannot read,
// which leaves the field as it would be without the hint (the server still refuses a wrong save).
const compiled = new WeakMap<FieldMeta, Condition | null>();

/** The signed-in login (App keeps it in the session), which `$user` reads. */
function signedInUser(): string {
  try {
    return sessionStorage.getItem("obop-user") ?? "";
  } catch {
    return "";
  }
}

/**
 * Whether the form shows `f` read-only on the saved record `row` by its `readonly_when`, for the
 * person `user` (by default the one signed in).
 */
export function readonlyByState(f: FieldMeta, row: Row | null, user = signedInUser()): boolean {
  const src = f.ui?.readonly_when;
  if (!src || !row) return false;
  let c = compiled.get(f);
  if (c === undefined) {
    c = compileCondition(src, { viewer: true });
    compiled.set(f, c);
  }
  return !!c && c(row, user);
}

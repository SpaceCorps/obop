// A text field that shows the label of an option (`shows_option_of`, D52 amendment): its value, when
// it equals an option of a select field, is shown with that option's label in the active language,
// while the condition `shows_option_when` (if any) holds on the record. Only what is drawn changes:
// the stored value, filters, sorting, grouping, export, import and the API are untouched, and an edit
// input shows the stored text.
import type { FieldMeta, ModuleMeta } from "./api.ts";
import { compileCondition, type Condition } from "./displayCondition.ts";

type Shown = {
  /** The select field whose option labels are used. */
  target: FieldMeta;
  /** The condition on the record; `null` when there is none. */
  when: Condition | null;
};

// Resolved when the metadata is loaded and kept beside the field, not in it, so that the metadata
// stays exactly what the server sent.
const resolved = new WeakMap<FieldMeta, Shown>();

/**
 * Link every field that names `shows_option_of` to the select field it names, and compile its
 * condition. A target the person cannot see (another model or group) or that is not a select leaves
 * the field showing what is stored; so does a condition this browser cannot read.
 */
export function resolveShownOptions(modules: ModuleMeta[]): void {
  const byName = new Map<string, FieldMeta>();
  for (const m of modules)
    for (const model of m.models)
      for (const f of model.fields) byName.set(`${model.name}.${f.name}`, f);
  for (const m of modules)
    for (const model of m.models)
      for (const f of model.fields) {
        if (!f.shows_option_of) continue;
        const target = byName.get(f.shows_option_of);
        if (!target || target.kind !== "select" || !target.options?.length) continue;
        const src = f.shows_option_when;
        resolved.set(f, {
          target,
          when: src === undefined ? null : (compileCondition(src) ?? (() => false)),
        });
      }
}

/**
 * The select field whose option label `value` is shown with, or `undefined` when the value is shown as
 * stored: the field does not name one, the value is not one of its options, or the condition does not
 * hold on `row` (or cannot be asked because there is no row).
 */
export function shownOptionTarget(
  f: FieldMeta,
  value: unknown,
  row?: Record<string, unknown>,
): FieldMeta | undefined {
  const s = resolved.get(f);
  if (!s || typeof value !== "string" || !s.target.options?.includes(value)) return undefined;
  if (s.when && !(row && s.when(row))) return undefined;
  return s.target;
}

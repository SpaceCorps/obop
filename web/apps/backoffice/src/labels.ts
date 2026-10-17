// How server metadata (modules, models, fields, select values, actions) is shown in the active
// language. Order of precedence: the server's own `labels[lang]` (when /api/meta carries them), then
// the client-side tables of the built-in modules (message contexts app, model, field, option, action),
// then the humanised English name. See docs/brand/LOCALIZATION.md.
import { getLang, labelFor, tx } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, ModuleMeta } from "./api.ts";
import { label, modelLabel } from "./format.ts";

/** The name of an app (module) as shown in the launcher, sidebar and palette. */
export function appTitle(m: Pick<ModuleMeta, "name" | "labels">): string {
  return labelFor(m, getLang(), tx("app", m.name));
}

export function modelTitle(m: Pick<ModelMeta, "name" | "labels">, moduleId: string): string {
  return labelFor(m, getLang(), tx("model", modelLabel(m.name, moduleId)));
}

/** A field's label for column headers, forms, filters and group-by. */
export function fieldTitle(f: Pick<FieldMeta, "name" | "labels"> & { kind?: string }): string {
  const en = label(f.name);
  const own = tx("field", en);
  // A text field called tax_id is an identifier, not a reference: "Tax ID", not "Tax".
  const fallback =
    f.kind && f.kind !== "ref" && f.name.endsWith("_id") && own === en ? `${en} ID` : own;
  return labelFor(f, getLang(), fallback);
}

/** The label of a field known only by its technical name (a saved search, a widget, a CSV header). */
export function fieldTitleByName(name: string, fields?: FieldMeta[]): string {
  const f = fields?.find((x) => x.name === name);
  return f ? fieldTitle(f) : tx("field", label(name));
}

/** The label of an action (a state transition or a command) on a record. */
export function actionTitle(c: Pick<Command, "id" | "labels">): string {
  return labelFor(c, getLang(), tx("action", label(c.id.split(".")[1] ?? c.id)));
}

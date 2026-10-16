import type { FieldMeta } from "./api.ts";
import { isEmpty, label, majorString, parseMoney, validDate } from "./format.ts";

/** What a form control holds: text for everything, a boolean for checkboxes. */
export type Edit = string | boolean;

export function toEdit(f: FieldMeta, v: unknown): Edit {
  if (f.kind === "bool") return v === true;
  if (isEmpty(v)) return "";
  if (f.kind === "money") return majorString(v);
  return String(v);
}

/** Turn what was typed into the wire value, or say why it cannot be one. `null` means empty. */
export function fromEdit(f: FieldMeta, e: Edit): { value: unknown } | { error: string } {
  if (f.kind === "bool") return { value: e === true };
  const s = String(e).trim();
  if (s === "") {
    return f.required ? { error: `${label(f.name)} is required` } : { value: null };
  }
  switch (f.kind) {
    case "number":
      return /^-?\d+$/.test(s)
        ? { value: parseInt(s, 10) }
        : { error: `${label(f.name)} must be a whole number` };
    case "money": {
      const m = parseMoney(s);
      return m === null ? { error: `${label(f.name)} must be an amount like 12.50` } : { value: m };
    }
    case "date":
      return validDate(s) ? { value: s } : { error: `${label(f.name)} must be a valid date` };
    case "select":
      return f.options?.includes(s)
        ? { value: s }
        : { error: `${label(f.name)} must be one of: ${f.options?.join(", ")}` };
    default:
      return { value: e as string };
  }
}

/** Validate a whole draft; returns errors by field name. */
export function validate(
  fields: FieldMeta[],
  values: (f: FieldMeta) => Edit,
  only?: (f: FieldMeta) => boolean,
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const f of fields) {
    if (only && !only(f)) continue;
    const r = fromEdit(f, values(f));
    if ("error" in r) errors[f.name] = r.error;
  }
  return errors;
}

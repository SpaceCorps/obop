import { fmt, t } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import { isEmpty, majorString, optionLabel, parseMoney, validDate } from "./format.ts";
import { fieldTitle } from "./labels.ts";

/** What a form control holds: text for everything, a boolean for checkboxes. */
export type Edit = string | boolean;

export function toEdit(f: FieldMeta, v: unknown, currency?: string): Edit {
  if (f.kind === "bool") return v === true;
  if (isEmpty(v)) return "";
  if (f.kind === "money") return majorString(v, currency);
  return String(v);
}

/** Turn what was typed into the wire value, or say why it cannot be one. `null` means empty. */
export function fromEdit(
  f: FieldMeta,
  e: Edit,
  currency?: string,
): { value: unknown } | { error: string } {
  const field = fieldTitle(f);
  if (f.kind === "bool") return { value: e === true };
  const s = String(e).trim();
  if (s === "") {
    return f.required ? { error: t("{field} is required", { field }) } : { value: null };
  }
  switch (f.kind) {
    case "number":
      return /^-?\d+$/.test(s)
        ? { value: parseInt(s, 10) }
        : { error: t("{field} must be a whole number", { field }) };
    case "money": {
      const m = parseMoney(s, currency);
      return m === null
        ? {
            error: t("{field} must be an amount like {example}", {
              field,
              example: fmt.moneyInput(1250, currency),
            }),
          }
        : { value: m };
    }
    case "date":
      return validDate(s) ? { value: s } : { error: t("{field} must be a valid date", { field }) };
    case "select":
      return f.options?.includes(s)
        ? { value: s }
        : {
            error: t("{field} must be one of: {options}", {
              field,
              options: (f.options ?? []).map(optionLabel).join(", "),
            }),
          };
    default:
      return { value: e as string };
  }
}

/** Validate a whole draft; returns errors by field name. */
export function validate(
  fields: FieldMeta[],
  values: (f: FieldMeta) => Edit,
  only?: (f: FieldMeta) => boolean,
  currency?: string,
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const f of fields) {
    if (only && !only(f)) continue;
    const r = fromEdit(f, values(f), currency);
    if ("error" in r) errors[f.name] = r.error;
  }
  return errors;
}

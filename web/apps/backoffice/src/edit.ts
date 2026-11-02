import { fmt, t } from "@obop/ui";
import { getTime, type FieldMeta } from "./api.ts";
import { canonicalDecimal, decimalInput } from "./decimals.ts";
import { isEmpty, majorString, optionLabel, parseMoney, validDate } from "./format.ts";
import { fromZonedInput, toZonedInput } from "./instants.ts";
import { fieldTitle } from "./labels.ts";
import { rangeError, textLimitError } from "./limits.ts";

/** What a form control holds: text for everything, a boolean for checkboxes. */
export type Edit = string | boolean;

export function toEdit(f: FieldMeta, v: unknown, currency?: string): Edit {
  if (f.kind === "bool") return v === true;
  if (isEmpty(v)) return "";
  if (f.kind === "money") return majorString(v, currency);
  if (f.kind === "decimal") return decimalInput(v, fmt.separators().decimal);
  if (f.kind === "datetime") return toZonedInput(v, getTime().zone);
  return String(v);
}

/**
 * Turn what was typed into the wire value, or say why it cannot be one. `null` means empty. A value
 * the person did not change (`unchanged`) is not held to the length and range limits of the field,
 * as on the server (D90): a record stored above a limit that was lowered later can still be saved.
 */
export function fromEdit(
  f: FieldMeta,
  e: Edit,
  currency?: string,
  unchanged = false,
): { value: unknown } | { error: string } {
  const field = fieldTitle(f);
  if (f.kind === "bool") return { value: e === true };
  const s = String(e).trim();
  if (s === "") {
    return f.required ? { error: t("{field} is required", { field }) } : { value: null };
  }
  switch (f.kind) {
    case "number": {
      if (!/^-?\d+$/.test(s)) return { error: t("{field} must be a whole number", { field }) };
      const n = parseInt(s, 10);
      const out = unchanged ? null : rangeError(field, f, n);
      return out ? { error: out } : { value: n };
    }
    case "money": {
      const m = parseMoney(s, currency);
      if (m === null)
        return {
          error: t("{field} must be an amount like {example}", {
            field,
            example: fmt.moneyInput(1250, currency),
          }),
        };
      const out = unchanged ? null : rangeError(field, f, m, currency);
      return out ? { error: out } : { value: m };
    }
    case "date":
      return validDate(s) ? { value: s } : { error: t("{field} must be a valid date", { field }) };
    case "decimal": {
      const scale = f.scale ?? 0;
      const digits = (f.precision ?? 38) - scale;
      const p = fmt.parseNumber(s, scale);
      const tooManyDigits = {
        error:
          scale === 0
            ? t("{field} must be a whole number", { field })
            : t("{field} must be a number with at most {scale} digits after the point", {
                field,
                scale,
              }),
      };
      if (!p) return tooManyDigits;
      // "1.234" in a field of two decimals could be 1,234 or a mistake: never guess for an exact
      // value. But a lone mark that is this locale's own decimal separator is a decimal point
      // with too many digits, and is told so, not offered a thousands reading it does not have.
      if (p.ambiguous && s.replace(/[\d\s+\-\u2212]/g, "") === fmt.separators().decimal)
        return tooManyDigits;
      if (p.ambiguous)
        return {
          error: t(
            "{field}: {text} could be read two ways; write it without thousands separators",
            {
              field,
              text: s,
            },
          ),
        };
      if (p.int !== "0" && p.int.length > digits)
        return {
          error: t("{field} has too many digits: at most {digits} before the point", {
            field,
            digits,
          }),
        };
      const dec = canonicalDecimal(p.negative, p.int, p.frac, scale);
      const out = unchanged ? null : rangeError(field, f, dec);
      return out ? { error: out } : { value: dec };
    }
    case "datetime": {
      const { zone } = getTime();
      const r = fromZonedInput(s, zone);
      if ("value" in r) return { value: r.value };
      return {
        error:
          r.error === "gap"
            ? t(
                "{field}: that time does not exist in {zone} (the clocks go forward); add an offset such as +01:00",
                { field, zone },
              )
            : r.error === "range"
              ? t("{field} must be between the years 0001 and 9999", { field })
              : r.error === "zone"
                ? t("{field}: this browser does not know the time zone {zone}", { field, zone })
                : t(
                    "{field} must be a date and time like {example}, in {zone} (or end with Z or an offset such as +02:00)",
                    {
                      field,
                      example: "2026-03-29 14:30",
                      zone,
                    },
                  ),
      };
    }
    case "select":
      return f.options?.includes(s)
        ? { value: s }
        : {
            error: t("{field} must be one of: {options}", {
              field,
              options: (f.options ?? []).map((o) => optionLabel(o, f)).join(", "),
            }),
          };
    default: {
      // The server counts what is sent, untrimmed (D90).
      const out = unchanged ? null : textLimitError(field, f, String(e));
      return out ? { error: out } : { value: e as string };
    }
  }
}

/**
 * Validate a whole draft; returns errors by field name. `unchanged` says which fields hold the
 * stored value, which are not held to the limits of their field (see `fromEdit`).
 */
export function validate(
  fields: FieldMeta[],
  values: (f: FieldMeta) => Edit,
  only?: (f: FieldMeta) => boolean,
  currency?: string,
  unchanged?: (f: FieldMeta) => boolean,
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const f of fields) {
    if (only && !only(f)) continue;
    const r = fromEdit(f, values(f), currency, unchanged?.(f) ?? false);
    if ("error" in r) errors[f.name] = r.error;
  }
  return errors;
}

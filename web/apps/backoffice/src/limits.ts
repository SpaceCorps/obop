// Field limits (D90) in the browser: the same rules the server applies, so that a form says what is
// wrong before it sends, in the person's language. The server stays the authority; its own refusal
// (serverMessage.ts) is shown in the same words when the browser could not know, for example a limit
// that changed after the page was loaded.
import { fmt, t } from "@obop/ui";
import type { FieldMeta } from "./api.ts";
import { compareDecimals, decimalInput } from "./decimals.ts";
import { majorString } from "./format.ts";

/**
 * The number of characters the server counts: Unicode code points, not UTF-16 units, so "å" and an
 * emoji are one each (`String.length` counts the emoji twice).
 */
export function codePoints(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      const d = s.charCodeAt(i + 1);
      if (d >= 0xdc00 && d <= 0xdfff) i++;
    }
    n++;
  }
  return n;
}

/** A bound or a value of a number, money or decimal field as the person reads it. */
export function showNumber(f: FieldMeta, raw: number | string, currency?: string): string {
  if (f.kind === "money") return majorString(Number(raw), currency);
  if (f.kind === "decimal") return decimalInput(String(raw), fmt.separators().decimal);
  return fmt.number(Number(raw));
}

export const tooLong = (field: string, max: number, have: number) =>
  t(
    "{field} must be at most {max, plural, one {# character} other {# characters}}; this has {have}",
    {
      field,
      max,
      have: fmt.number(have),
    },
  );

export const tooShort = (field: string, min: number, have: number) =>
  t(
    "{field} must be at least {min, plural, one {# character} other {# characters}}; this has {have}",
    {
      field,
      min,
      have: fmt.number(have),
    },
  );

export const tooSmall = (field: string, min: string, value: string) =>
  t("{field} must be at least {min}; this is {value}", { field, min, value });

export const tooBig = (field: string, max: string, value: string) =>
  t("{field} must be at most {max}; this is {value}", { field, max, value });

/** Why a text breaks the length limits of its field, or `null`. Counts the text as typed. */
export function textLimitError(field: string, f: FieldMeta, s: string): string | null {
  const max = f.max_length;
  const min = f.min_length;
  if (max === undefined && min === undefined) return null;
  // A string of n UTF-16 units has between n/2 and n characters: most are decided without counting.
  const sureMax = max === undefined || s.length <= max;
  const sureMin = min === undefined || Math.ceil(s.length / 2) >= min;
  if (sureMax && sureMin) return null;
  const have = codePoints(s);
  if (max !== undefined && have > max) return tooLong(field, max, have);
  if (min !== undefined && have < min) return tooShort(field, min, have);
  return null;
}

/**
 * Why a number breaks the `min` or `max` of its field, or `null`. `value` is the wire value: a
 * number for `number` and `money` (minor units), the canonical string for `decimal`.
 */
export function rangeError(
  field: string,
  f: FieldMeta,
  value: number | string,
  currency?: string,
): string | null {
  const order = (bound: number | string) =>
    f.kind === "decimal"
      ? compareDecimals(String(value), String(bound))
      : Math.sign(Number(value) - Number(bound));
  if (f.min !== undefined && order(f.min) < 0)
    return tooSmall(field, showNumber(f, f.min, currency), showNumber(f, value, currency));
  if (f.max !== undefined && order(f.max) > 0)
    return tooBig(field, showNumber(f, f.max, currency), showNumber(f, value, currency));
  return null;
}

/**
 * How full a text field is, for its counter: `null` while the text is far from the limit (a counter
 * on every name would only be noise), else the characters used and the limit.
 */
export function counterState(
  f: FieldMeta,
  s: string,
): { used: number; max: number; over: boolean } | null {
  const max = f.max_length;
  if (max === undefined) return null;
  // Fewer UTF-16 units than four fifths of the limit cannot be four fifths of it in characters.
  if (s.length * 5 < max * 4) return null;
  const used = codePoints(s);
  return used * 5 >= max * 4 ? { used, max, over: used > max } : null;
}

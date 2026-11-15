import { en, type MessageKey, type Messages } from "./en.ts";

export type { MessageKey, Messages };

const catalogue: Messages = en;

/** The language of the catalogue and of the numbers and dates the app formats (BCP 47). */
export const LOCALE = "en";

type Params = Record<string, string | number>;

function fill(text: string, params: Params): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}

/** A sentence of the catalogue with its `{placeholders}` filled. */
export function t(key: MessageKey, params: Params = {}): string {
  return fill(catalogue[key], params);
}

/** The keys that have plural forms: `name` where the catalogue holds `name.one`, `name.other`, and so on. */
export type PluralKey = MessageKey extends infer K
  ? K extends `${infer Base}.other`
    ? Base
    : never
  : never;

/** The plural rules of English, for a runtime that has no `Intl.PluralRules` (Hermes on Android has none). */
export const englishPlural = (count: number): "one" | "other" => (count === 1 ? "one" : "other");

let rules: Intl.PluralRules | null = null;

/** The plural category of a count in the language of the catalogue. */
function pluralCategory(count: number): string {
  if (typeof Intl !== "undefined" && typeof Intl.PluralRules === "function") {
    try {
      rules ??= new Intl.PluralRules(LOCALE);
      return rules.select(count);
    } catch {
      // Fall through to the table.
    }
  }
  return englishPlural(count);
}

/** A number in the way the language writes it (decimal separator, grouping). */
export function formatNumber(value: number, maximumFractionDigits = 0): string {
  try {
    return new Intl.NumberFormat(LOCALE, { maximumFractionDigits }).format(value);
  } catch {
    return String(Number(value.toFixed(maximumFractionDigits)));
  }
}

/**
 * A sentence that depends on a count: the form is chosen by the plural rules of the language
 * (`Intl.PluralRules`, or the table of English where the runtime has none; English has `one` and
 * `other`, Polish has more) and `{count}` is the number written the way the language writes it. A
 * language lists only the forms it needs, and `other` is the one every language has.
 */
export function tp(key: PluralKey, count: number, params: Params = {}): string {
  const exact = `${key}.${pluralCategory(count)}`;
  const text =
    (catalogue as Record<string, string>)[exact] ?? catalogue[`${key}.other` as MessageKey];
  return fill(text, { ...params, count: formatNumber(count) });
}

/** A date and time for a driver, in the language and time zone of the phone. */
export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  try {
    return new Intl.DateTimeFormat(LOCALE, { dateStyle: "medium", timeStyle: "short" }).format(
      date,
    );
  } catch {
    return date.toLocaleString(LOCALE);
  }
}

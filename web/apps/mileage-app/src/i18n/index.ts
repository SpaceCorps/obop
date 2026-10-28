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

const plurals = new Intl.PluralRules(LOCALE);

/**
 * A sentence that depends on a count: the form is chosen by the plural rules of the language
 * (`Intl.PluralRules`; English has `one` and `other`, Polish has more) and `{count}` is the number
 * written the way the language writes it. A language lists only the forms it needs, and `other`
 * is the one every language has.
 */
export function tp(key: PluralKey, count: number, params: Params = {}): string {
  const exact = `${key}.${plurals.select(count)}`;
  const text =
    (catalogue as Record<string, string>)[exact] ?? catalogue[`${key}.other` as MessageKey];
  return fill(text, { ...params, count: new Intl.NumberFormat(LOCALE).format(count) });
}

/** A number in the way the language writes it (decimal separator, grouping). */
export const formatNumber = (value: number, maximumFractionDigits = 0): string =>
  new Intl.NumberFormat(LOCALE, { maximumFractionDigits }).format(value);

/** A date and time for a driver, in the language and time zone of the phone. */
export const formatDateTime = (iso: string): string =>
  new Intl.DateTimeFormat(LOCALE, { dateStyle: "medium", timeStyle: "short" }).format(
    new Date(iso),
  );

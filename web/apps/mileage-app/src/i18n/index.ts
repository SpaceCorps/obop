import type { MessageKey, PluralKey } from "./en.ts";
import { LANGUAGES, type Language, type LanguageCode } from "./languages.ts";

export type { MessageKey, PluralKey };
export type { LanguageCode };

/**
 * The active language: what `t`, `tp`, `tOn`, `formatNumber` and `formatDateTime` read. It is
 * English until `setLanguage` is called (`language.ts` decides which language and calls it; the
 * tests rely on the default). The screens read these functions while they render, so a change is
 * shown by rendering again: `App.tsx` renders the whole tree again when the language changes.
 */
let active: Language = LANGUAGES.en;
const listeners = new Set<() => void>();

export function getLanguage(): LanguageCode {
  return active.code;
}

/** Makes `code` the language of the sentences, numbers and dates, and tells the listeners if it changed. */
export function setLanguage(code: LanguageCode): void {
  const next = LANGUAGES[code];
  if (next === active) return;
  active = next;
  for (const listener of listeners) listener();
}

/** `listener` is called after the language changed. Returns the function that stops it. */
export function onLanguageChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

type Params = Record<string, string | number>;

function fill(text: string, params: Params): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}

/** The text of a key in the active language; the English one if the active catalogue lacks it (the tests say it never does). */
function lookup(key: string): string {
  return active.messages[key] ?? LANGUAGES.en.messages[key] ?? key;
}

/** A sentence of the catalogue with its `{placeholders}` filled. */
export function t(key: MessageKey, params: Params = {}): string {
  return fill(lookup(key), params);
}

/**
 * A sentence of the catalogue for the system the app runs on: `name.ios` on iOS where the
 * catalogue has one (a sentence that names something Android has and iOS has not), else `name`.
 * The system is passed in (the screens pass `Platform.OS`, `ui/os-text.ts`), so this stays free of
 * React Native and runs in `vp test`.
 */
export function tOn(os: string, key: MessageKey, params: Params = {}): string {
  const iosKey = `${key}.ios`;
  if (os === "ios" && iosKey in active.messages) return fill(lookup(iosKey), params);
  return fill(lookup(key), params);
}

/** A number in the way the language writes it (decimal separator, grouping). */
export function formatNumber(value: number, maximumFractionDigits = 0): string {
  try {
    return new Intl.NumberFormat(active.tag, { maximumFractionDigits }).format(value);
  } catch {
    // No Intl.NumberFormat, or none for this language: the language's own symbols by hand.
    return plainNumber(value, maximumFractionDigits, active);
  }
}

/** `formatNumber` without `Intl`: rounds, drops trailing zeros, groups the thousands with the language's symbols. */
function plainNumber(value: number, maximumFractionDigits: number, language: Language): string {
  const fixed = Number(value.toFixed(maximumFractionDigits));
  const [whole = "0", fraction] = String(Math.abs(fixed)).split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, language.symbols.group);
  const sign = fixed < 0 ? "-" : "";
  return sign + grouped + (fraction ? language.symbols.decimal + fraction : "");
}

/**
 * A sentence that depends on a count: the form is chosen by the plural rules of the language
 * (`plural` of `languages.ts`: English and Swedish have `one` and `other`, Polish would have more)
 * and `{count}` is the number written the way the language writes it. A language lists only the
 * forms it needs, and `other` is the one every language has.
 */
export function tp(key: PluralKey, count: number, params: Params = {}): string {
  const text = active.messages[`${key}.${active.plural(count)}`] ?? lookup(`${key}.other`);
  return fill(text, { ...params, count: formatNumber(count) });
}

/** A date and time for a driver, in the language and time zone of the phone. */
export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  try {
    return new Intl.DateTimeFormat(active.tag, { dateStyle: "medium", timeStyle: "short" }).format(
      date,
    );
  } catch {
    try {
      return date.toLocaleString(active.tag);
    } catch {
      return iso;
    }
  }
}

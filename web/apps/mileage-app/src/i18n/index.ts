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

/**
 * Whether a formatter that `Intl` made speaks the language. A valid tag the engine has no data for
 * does not throw: the constructor resolves to its default locale (`en-US`) and formats in that, so
 * a `catch` alone only sees an `Intl` that is absent. Only the language part is compared (`sv`
 * against `sv-SE`). A formatter that does not say what it resolved to is trusted.
 */
function speaks(resolved: unknown, tag: string): boolean {
  if (typeof resolved !== "string" || resolved === "") return true;
  return resolved.toLowerCase().split(/[-_]/)[0] === tag.toLowerCase().split(/[-_]/)[0];
}

/**
 * A number in the way the language writes it (decimal separator, grouping). Through
 * `Intl.NumberFormat` when the engine has it AND has data for the language; otherwise by hand with
 * the symbols of the language (`plainNumber`), so a Swedish app does not show an English number.
 */
export function formatNumber(value: number, maximumFractionDigits = 0): string {
  try {
    const format = new Intl.NumberFormat(active.tag, { maximumFractionDigits });
    if (speaks(format.resolvedOptions().locale, active.tag)) return format.format(value);
  } catch {
    // No Intl.NumberFormat at all.
  }
  return plainNumber(value, maximumFractionDigits, active);
}

/**
 * `formatNumber` without `Intl`: rounds half away from zero on the decimal text of the value (as
 * `Intl` does: 8.35 with one digit is 8.4, where `toFixed` gives 8.3), drops trailing zeros,
 * groups the thousands and writes the minus sign with the symbols of the language. A number the
 * app never shows is not formatted: one that is not finite, or too large to scale (1e21 and
 * more), is written as JavaScript writes it; one smaller than 1e-6 is "0" (JavaScript writes such
 * a number with an exponent, and `Intl` writes "0"; a negative that rounds to nothing is "0" too,
 * where `Intl` writes "-0").
 */
export function plainNumber(
  value: number,
  maximumFractionDigits: number,
  language: Language,
): string {
  if (!Number.isFinite(value)) return String(value);
  if (Math.abs(value) < 1e-6) return "0";
  const digits = Math.min(20, Math.max(0, Math.trunc(maximumFractionDigits)));
  const shifted = Number(`${Math.abs(value)}e${digits}`);
  if (!Number.isFinite(shifted) || shifted >= 1e21) return String(value);
  const scaled = String(Math.round(shifted)).padStart(digits + 1, "0");
  const whole = scaled.slice(0, scaled.length - digits);
  const fraction = scaled.slice(scaled.length - digits).replace(/0+$/, "");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, language.symbols.group);
  const sign = value < 0 && /[1-9]/.test(scaled) ? language.symbols.minus : "";
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

/**
 * A date and time for a driver, in the language and time zone of the phone: `Intl.DateTimeFormat`
 * when the engine has it and has data for the language (Swedish: "15 okt. 2026 11:00" on Node).
 * Without that, `2026-10-15 11:00` in the time zone of the phone, written by hand: the same on
 * every engine, and the order Swedish writes dates in. What Hermes does with `sv` is not known
 * (docs/spec/mobile.md 8.9). A value that is not a date is returned as it was given.
 */
export function formatDateTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  try {
    const format = new Intl.DateTimeFormat(active.tag, { dateStyle: "medium", timeStyle: "short" });
    if (speaks(format.resolvedOptions().locale, active.tag)) return format.format(date);
  } catch {
    // No Intl.DateTimeFormat at all.
  }
  return plainDateTime(date);
}

function plainDateTime(date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())} ${two(date.getHours())}:${two(date.getMinutes())}`;
}

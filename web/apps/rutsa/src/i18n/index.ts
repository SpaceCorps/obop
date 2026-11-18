import { en, type MessageKey, type PluralKey } from "./en.ts";

export type { MessageKey, PluralKey };

/**
 * The sentences of the app. English only in stage 1; Swedish (stage 2) adds a second catalogue
 * and a way to choose it, as `mileage-app` has (`src/i18n/languages.ts`, `language.ts`).
 */
const messages: Readonly<Record<string, string>> = en;

/** The language the sentences are in; `core/present.ts` formats numbers and times for it too. */
export const LANGUAGE = "en";

type Params = Record<string, string | number>;

/** Fills `{name}` placeholders; one that has no value is left as it is, so it shows in a test. */
function fill(text: string, params: Params): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}

/** A sentence of the catalogue with its `{placeholders}` filled. */
export function t(key: MessageKey, params: Params = {}): string {
  return fill(messages[key] ?? key, params);
}

/**
 * The plural form of a count in the language: `Intl.PluralRules` where the engine has it, else the
 * rule of English (1 is `one`, everything else `other`).
 */
function pluralOf(count: number): string {
  try {
    return new Intl.PluralRules(LANGUAGE).select(count);
  } catch {
    return count === 1 ? "one" : "other";
  }
}

/** A sentence that depends on `count`, in the form the language uses for it; `{count}` is filled. */
export function tp(key: PluralKey, count: number, params: Params = {}): string {
  const text = messages[`${key}.${pluralOf(count)}`] ?? messages[`${key}.other`] ?? key;
  return fill(text, { ...params, count });
}

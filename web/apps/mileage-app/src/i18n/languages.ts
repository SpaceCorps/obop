import { en, type PluralCategory } from "./en.ts";
import { sv } from "./sv.ts";

/** The languages of the app. Adding one: its catalogue, one entry below, and the tests tell what is missing. */
export const LANGUAGE_CODES = ["en", "sv"] as const;
export type LanguageCode = (typeof LANGUAGE_CODES)[number];

/**
 * How a language writes numbers; used only where the runtime has no `Intl.NumberFormat` (or none
 * with data for the language). The tests compare them with `Intl.NumberFormat` on Node. Not here:
 * `minimumGroupingDigits` (Spanish and Polish write 1234 without a separator, CLDR), which a
 * language that needs it has to add.
 */
export interface NumberSymbols {
  decimal: string;
  group: string;
  /** The minus sign: the hyphen in English, U+2212 in Swedish (CLDR). */
  minus: string;
}

export interface Language {
  code: LanguageCode;
  /** The BCP 47 tag that `Intl` formats numbers and dates with. */
  tag: string;
  /** The name of the language in that language: shown as it is in the language choice, never translated. */
  name: string;
  /**
   * The plural categories the sentences of this language have (CLDR; `other` is always one of
   * them). The tests compare them with `Intl.PluralRules` on Node and check that the catalogue has
   * exactly these forms.
   */
  categories: readonly PluralCategory[];
  /**
   * The plural category of a count. The rule written out in code, not `Intl.PluralRules`: Hermes
   * on Android has no `Intl.PluralRules` (the app crashed at start, docs/spec/mobile.md 8.7), and
   * the tests compare this function with `Intl.PluralRules` on Node for every count from 0 to 1000.
   */
  plural(count: number): PluralCategory;
  symbols: NumberSymbols;
  messages: Readonly<Record<string, string>>;
  /** Keys whose text is the same as the English one on purpose (a name, a word both languages write alike). */
  identical: readonly string[];
}

/** English and Swedish: `one` for exactly one, `other` for everything else (CLDR, integers). */
const oneOther = (count: number): PluralCategory => (Math.abs(count) === 1 ? "one" : "other");

export const LANGUAGES: Readonly<Record<LanguageCode, Language>> = {
  en: {
    code: "en",
    tag: "en",
    name: "English",
    categories: ["one", "other"],
    plural: oneOther,
    symbols: { decimal: ".", group: ",", minus: "-" },
    messages: en,
    identical: [],
  },
  sv: {
    code: "sv",
    tag: "sv",
    name: "Svenska",
    categories: ["one", "other"],
    plural: oneOther,
    // CLDR: a comma for the decimal mark, a no-break space between thousands.
    symbols: { decimal: ",", group: "\u00a0", minus: "\u2212" },
    messages: sv,
    // "Start", "Server" and "Problem" are written alike in both languages; "Mileage" is the name of the app in every language.
    identical: ["account.server", "home.start", "home.title", "words.danger"],
  },
};

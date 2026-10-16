/** UI languages that ship a catalogue. English is the source language and is always inline. */
export const LANGS = ["en", "de", "fr", "pl", "fi", "sv", "es"] as const;
/** Pseudo-locales for testing: accented and 40% longer; the second also mirrors the layout. */
export const PSEUDO_LANGS = ["en-XA", "ar-XB"] as const;

export type RealLang = (typeof LANGS)[number];
export type Lang = RealLang | (typeof PSEUDO_LANGS)[number];

export type LangInfo = {
  /** The name of the language in that language; never translated, so a person can always find it. */
  native: string;
  dir: "ltr" | "rtl";
  /** The format locale used when the person has not chosen a region and the browser does not say. */
  format: string;
};

export const LANG_INFO: Record<Lang, LangInfo> = {
  en: { native: "English", dir: "ltr", format: "en-GB" },
  de: { native: "Deutsch", dir: "ltr", format: "de-DE" },
  fr: { native: "Français", dir: "ltr", format: "fr-FR" },
  pl: { native: "Polski", dir: "ltr", format: "pl-PL" },
  fi: { native: "Suomi", dir: "ltr", format: "fi-FI" },
  sv: { native: "Svenska", dir: "ltr", format: "sv-SE" },
  es: { native: "Español", dir: "ltr", format: "es-ES" },
  "en-XA": { native: "Pseudo (accented)", dir: "ltr", format: "en-GB" },
  "ar-XB": { native: "Pseudo (mirrored)", dir: "rtl", format: "en-GB" },
};

export const isLang = (s: string | null | undefined): s is Lang =>
  !!s && Object.hasOwn(LANG_INFO, s);
export const isPseudo = (l: Lang) => (PSEUDO_LANGS as readonly string[]).includes(l);

/** Region (format) choices: number, date and currency formats, independent of the language. */
export const REGIONS = [
  "en-GB",
  "en-US",
  "en-IE",
  "de-DE",
  "de-AT",
  "de-CH",
  "fr-FR",
  "fr-BE",
  "fr-CH",
  "pl-PL",
  "fi-FI",
  "sv-SE",
  "es-ES",
  "es-MX",
] as const;

/** Map a browser language tag ("de-AT", "pl") to a shipped language, or null. */
export function matchLang(tag: string | null | undefined): Lang | null {
  if (!tag) return null;
  if (isLang(tag)) return tag;
  const base = tag.toLowerCase().split(/[-_]/)[0];
  return (LANGS as readonly string[]).includes(base!) ? (base as Lang) : null;
}

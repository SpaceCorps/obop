import { getLang } from "./store.ts";

/** What a server record of metadata (module, model, field) may carry: language code -> display label. */
export type Translatable = {
  name?: string;
  description?: string;
  labels?: Record<string, string> | null;
};

/**
 * The label to show for a piece of server metadata: `labels[lang]`, else `labels[base language]`
 * ("de" for "de-AT"), else the fallback (the client-side translation or the humanised English name).
 */
export function labelFor(
  obj: Translatable | null | undefined,
  lang: string = getLang(),
  fallback?: string,
): string {
  const l = obj?.labels;
  const hit = l?.[lang] ?? l?.[lang.split("-")[0]!];
  if (typeof hit === "string" && hit.trim()) return hit;
  return fallback ?? obj?.description ?? obj?.name ?? "";
}

/** The description of server metadata in the active language (`descriptions[lang]`, else English). */
export function describeFor(
  obj: { description?: string; descriptions?: Record<string, string> | null } | null | undefined,
  lang: string = getLang(),
): string {
  const d = obj?.descriptions;
  const hit = d?.[lang] ?? d?.[lang.split("-")[0]!];
  return typeof hit === "string" && hit.trim() ? hit : (obj?.description ?? "");
}

/**
 * A noun inside a sentence: lower-case first letter in languages that write common nouns small
 * ("New lead", "Nouveau devis"), unchanged in German (nouns are capitalised) and for acronyms (CRM).
 */
export function inSentence(noun: string): string {
  const lang = getLang();
  if (lang === "de" || !/^\p{Lu}\p{Ll}/u.test(noun)) return noun;
  return noun.charAt(0).toLocaleLowerCase(lang) + noun.slice(1);
}

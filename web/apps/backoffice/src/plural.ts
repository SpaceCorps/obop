import { getLang, t } from "@obop/ui";
import type { ModelMeta } from "./api.ts";

// The plural name of a model, for page headings, empty states and menus ("Sale orders").

/** Words that have no English plural of their own. */
const UNCOUNTABLE =
  /(equipment|staff|information|data|feedback|software|knowledge|personnel|news)$/i;

/** A humanised English name in the plural: "Sale order" → "Sale orders", "Party" → "Parties". */
export function englishPlural(name: string): string {
  if (!name || UNCOUNTABLE.test(name)) return name;
  if (/[^aeiou]y$/i.test(name)) return `${name.slice(0, -1)}ies`;
  if (/is$/i.test(name)) return `${name.slice(0, -2)}es`;
  if (/(ss|us|ch|sh|x|z)$/i.test(name)) return `${name}es`;
  if (/s$/i.test(name)) return name;
  return `${name}s`;
}

/**
 * The model's name in the plural, for the page heading and the empty states: the D95 `plural` in
 * the active language; else in English a plural of the English name; else the name as it is.
 */
export function pluralTitle(model: ModelMeta, title: string): string {
  const lang = getLang();
  const own = model.plural_labels?.[lang];
  if (own) return own;
  if (lang !== "en") return title;
  return model.plural ?? englishPlural(title);
}

/**
 * The heading of a lines table inside a record: the child model's plural where its package
 * declares one for the active language ("Components", "Order lines"); else "Lines" for a model
 * named `*_line(s)`; else the model's name.
 */
export function linesHeading(model: ModelMeta, title: string): string {
  const lang = getLang();
  const own = model.plural_labels?.[lang] ?? model.plural_labels?.[lang.split("-")[0]!];
  if (own?.trim()) return own;
  if (/^en\b/i.test(lang) && model.plural?.trim()) return model.plural;
  return /_lines?$/.test(model.name) ? t("Lines") : title;
}

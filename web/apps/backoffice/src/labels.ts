// How server metadata (modules, models, fields, select values, actions) is shown in the active
// language. Order of precedence: the server's own `labels[lang]` (when /api/meta carries them, for
// English too: a package may ship `packages/labels/en`, D95), then the client-side tables of the
// built-in modules (message contexts app, model, field, option, action), then the humanised English
// name, which keeps acronyms in capitals ("SEO title", "VAT", "IBAN"). See
// docs/brand/LOCALIZATION.md.
import { getLang, labelFor, tx } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, ModuleMeta } from "./api.ts";
import { label, modelLabel } from "./format.ts";

/** Words that are written in capitals, whatever the technical name says. */
const ACRONYMS: Record<string, string> = Object.fromEntries(
  [
    "API",
    "BIC",
    "BOM",
    "CSV",
    "DPA",
    "EAN",
    "GPS",
    "HTML",
    "IBAN",
    "ID",
    "IP",
    "MFA",
    "OAuth",
    "OIDC",
    "PDF",
    "POS",
    "SEO",
    "SKU",
    "SLA",
    "SMS",
    "SSO",
    "URL",
    "UTC",
    "VAT",
    "VIN",
  ].map((w) => [w.toLowerCase(), w]),
);

/** "Seo title" reads "SEO title", "Bank iban" reads "Bank IBAN", "Sso provider" "SSO provider". */
export function keepAcronyms(s: string): string {
  return s.replace(/[A-Za-z]+/g, (w) => ACRONYMS[w.toLowerCase()] ?? w);
}

/** The English words of a technical name, acronyms kept, when no table translated them. */
const humanised = (own: string, en: string) => (own === en ? keepAcronyms(en) : own);

/** Whether the active language is English (where the humanised names are the words shown). */
const english = () => /^en\b/i.test(getLang());

/** The name of an app (module) as shown in the launcher, sidebar and palette. */
export function appTitle(m: Pick<ModuleMeta, "name" | "labels">): string {
  return labelFor(m, getLang(), tx("app", m.name));
}

export function modelTitle(m: Pick<ModelMeta, "name" | "labels">, moduleId: string): string {
  const en = modelLabel(m.name, moduleId);
  return labelFor(m, getLang(), humanised(tx("model", en), en));
}

/** "Entry" becomes "Entries", "Tax" "Taxes", "BOM" "BOMs". English only. */
export function englishPlural(s: string): string {
  const m = /^(.*?)(\S+)$/.exec(s);
  if (!m) return s;
  const head = m[1]!;
  const word = m[2]!;
  const lower = word.toLowerCase();
  const irregular: Record<string, string> = { person: "people", child: "children" };
  if (irregular[lower]) return head + irregular[lower];
  if (/^[A-Z]{2,}$/.test(word)) return `${head}${word}s`;
  if (/[^aeiou]y$/i.test(word)) return `${head}${word.slice(0, -1)}ies`;
  if (/(s|x|z|ch|sh)$/i.test(word)) return `${head}${word}es`;
  return `${head}${word}s`;
}

/**
 * The name of several records of a model ("Journal entries"): the server's `plural_labels` for
 * the active language, its `plural` in English, the English plural of the name in English, and
 * otherwise the name itself (a language without a plural table keeps the singular).
 */
export function modelPlural(
  m: Pick<ModelMeta, "name" | "labels" | "plural" | "plural_labels">,
  moduleId: string,
): string {
  const lang = getLang();
  const own = m.plural_labels?.[lang] ?? m.plural_labels?.[lang.split("-")[0]!];
  if (typeof own === "string" && own.trim()) return own;
  if (english()) {
    if (m.plural?.trim()) return m.plural;
    return englishPlural(modelTitle(m, moduleId));
  }
  return modelTitle(m, moduleId);
}

/** A number kept in metres that is shown in kilometres: its name drops the unit ("Distance"). */
const shownInKm = (f: { name: string; kind?: string; ui?: FieldMeta["ui"] }) =>
  (f.kind === "number" || f.kind === "decimal") &&
  (f.ui?.display === "km_from_m" || (!f.ui?.display && /(distance|length)_m$/.test(f.name)));

/** How a number field is read, by its hint or by the name rules of presentation.ts `displayOf`. */
function numberDisplay(f: { name: string; kind?: string; ui?: FieldMeta["ui"] }): string {
  if (f.kind !== "number" && f.kind !== "decimal") return "";
  if (f.ui?.display) return f.ui.display;
  if (/(^|_)(start|end|from|to)_minute$|_minute_of_day$|^minute_of_day$/.test(f.name))
    return "time_of_day";
  if (/(^|_)(duration|minutes)$|_minutes$/.test(f.name)) return "duration_minutes";
  return "";
}

/**
 * The English name of a number that is shown in another unit drops the stored unit: "Distance"
 * (shown in km), "Start time" (a time of day), "Resolution time" (a duration), "Duration".
 */
function withoutUnit(text: string, f: { name: string; kind?: string; ui?: FieldMeta["ui"] }) {
  const display = numberDisplay(f);
  if (shownInKm(f)) return text.replace(/ m$/, "");
  if (display === "time_of_day") return text.replace(/ minute( of day)?$/i, " time");
  if (display === "duration_minutes")
    return /^minutes$/i.test(text) ? "Duration" : text.replace(/ minutes$/i, " time");
  return text;
}

/** A field's label for column headers, forms, filters and group-by. */
export function fieldTitle(
  f: Pick<FieldMeta, "name" | "labels"> & { kind?: string; ui?: FieldMeta["ui"] },
): string {
  const en = label(f.name);
  const own = tx("field", en);
  // A text field called tax_id is an identifier, not a reference: "Tax ID", not "Tax".
  let fallback =
    f.kind && f.kind !== "ref" && f.name.endsWith("_id") && own === en ? `${en} ID` : own;
  if (fallback === en || fallback === `${en} ID`) {
    fallback = withoutUnit(keepAcronyms(fallback), f);
  }
  return labelFor(f, getLang(), fallback);
}

/** The label of a field known only by its technical name (a saved search, a widget, a CSV header). */
export function fieldTitleByName(name: string, fields?: FieldMeta[]): string {
  const f = fields?.find((x) => x.name === name);
  if (f) return fieldTitle(f);
  const en = label(name);
  return humanised(tx("field", en), en);
}

/** The label of an action (a state transition or a command) on a record. */
export function actionTitle(c: Pick<Command, "id" | "labels">): string {
  const en = label(c.id.split(".")[1] ?? c.id);
  return labelFor(c, getLang(), humanised(tx("action", en), en));
}

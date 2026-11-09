import type { Messages } from "../../i18n/types.ts";
import { agentView, appName, loadLabels, type App, type Model } from "./apps.ts";
import { GITHUB } from "./paths.ts";

/**
 * The plain-words layer of an app page: what the top of the page shows (facts, what you can do, what
 * it keeps, what is safe, who it works with, the information list). Built once per app and language
 * from the same manifest data as the technical tables, so nothing here is written by hand, and
 * shared by the page (through its front matter) and by the Markdown mirror, so the two cannot drift.
 */
export interface Fact {
  label: string;
  value: string;
}
export interface InfoRow {
  label: string;
  value: string;
  /** The value is a link to this address. */
  href?: string;
  /** The value is a code-like token (a requirement, a path). */
  mono?: boolean;
  /** A plain-words remark after the value. */
  note?: string;
}
export interface Card {
  /** A small line above the title: the thing the action works on. */
  kicker?: string;
  title: string;
  text: string;
  /** The quiet "a person approves when an agent asks" mark. */
  approval?: boolean;
  /** "en" when the words are the English manifest's on a translated page; absent otherwise. */
  lang?: "en";
}
export interface AppLink {
  id: string;
  name: string;
  slug: string;
}
export interface AppView {
  /** The category, as the quiet line under the tagline. */
  category: string;
  facts: Fact[];
  /** The first sentence of the purpose, set large, and the rest, set small (empty for one sentence). */
  purposeLead: string;
  purposeRest: string;
  /** The rest is long: the page folds it under "Read more". */
  purposeFold: boolean;
  /** One card per workflow action; none when records are created and edited directly. */
  actions: Card[];
  /** One card per model: what the app keeps. */
  models: Card[];
  safe: string[];
  worksWith: AppLink[];
  /** Other apps of the same category, at most six. */
  related: AppLink[];
  info: InfoRow[];
  /** The command that installs a marketplace app; null for a built-in app. */
  command: string | null;
}

const ACRONYMS: Record<string, string> = { bom: "BOM", csv: "CSV", oauth: "OAuth", sla: "SLA" };

/** `reset_to_draft` becomes "reset to draft"; the few acronyms in the manifests keep their capitals. */
export const words = (id: string): string =>
  id
    .split("_")
    .map((w) => ACRONYMS[w] ?? w)
    .join(" ");
export const capital = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);
/**
 * The first letter in lower case, for a title that continues a sentence ("Ticket: close"). A word that
 * starts with an acronym or a camel-case name keeps its capitals ("OAuth start", "BOM explode"): it
 * is left alone when its second letter is a capital too.
 */
export const lowerFirst = (s: string): string =>
  s.length > 1 && s.charAt(1) !== s.charAt(1).toLowerCase()
    ? s
    : s.charAt(0).toLowerCase() + s.slice(1);

/**
 * A model's name in plain English: the one `data/apps.ts` gives it where the technical name reads
 * oddly, else its name without the module prefix (`helpdesk_ticket` in the Helpdesk app is a "Ticket").
 */
export function modelLabel(app: App, model: Model): string {
  const plain = app.modelNames[model.name];
  if (plain) return plain;
  const prefix = `${app.id}_`;
  const bare = model.name.startsWith(prefix) ? model.name.slice(prefix.length) : model.name;
  return capital(words(bare || model.name));
}

const fill = (s: string, vars: Record<string, string>) =>
  s.replace(/\{(\w+)\}/g, (_m, k: string) => vars[k] ?? "");

export const wordCount = (s: string): number => (s.trim() ? s.trim().split(/\s+/).length : 0);
/** The large lead stays short enough to read at a glance; the quiet rest folds when it is a wall. */
export const LEAD_WORDS = 25;
export const FOLD_WORDS = 40;

/**
 * The first sentence of a purpose text and the rest. A sentence ends at a full stop, question or
 * exclamation mark followed by a space and a capital; the purposes are hand-written and checked by
 * a test, so the rule can stay this simple.
 */
export function splitPurpose(text: string): { lead: string; rest: string } {
  const m = /[.!?](?=\s+[A-Z0-9"(])/.exec(text);
  let lead = m ? text.slice(0, m.index + 1) : text;
  let rest = m ? text.slice(m.index + 1).trim() : "";
  // A long first sentence is cut at its first colon or semicolon: the cut ends in a full stop and
  // what follows opens the quiet paragraph. The words stay the manifest owner's; only the place
  // where the big type ends is decided here.
  if (wordCount(lead) > LEAD_WORDS) {
    const cut = /[:;]/.exec(lead);
    if (cut && cut.index > 0) {
      const tail = lead.slice(cut.index + 1).trim();
      lead = `${lead.slice(0, cut.index)}.`;
      rest = [capital(tail), rest].filter(Boolean).join(" ");
    }
  }
  return { lead, rest };
}

/** `^0.2` is "0.2" in plain words; any other range stays as the manifest wrote it. */
function plainCompat(range: string, template: string): string | null {
  const m = /^\^0\.(\d+)(?:\.\d+)?$/.exec(range);
  return m ? fill(template, { version: `0.${m[1]}` }) : null;
}

export function appView(app: App, m: Messages, all: App[], locale = "en"): AppView {
  const a = m.apps;
  const s = a.store;
  const meta = app.meta;
  const category = a.categories[app.category];
  const maturity =
    a.meta.maturities[meta.maturity as keyof typeof a.meta.maturities] ?? meta.maturity;
  const developers = [meta.author, ...meta.authors].filter(Boolean).join(", ");

  // An app is called what the product calls it in the page language (packages/labels), else by its
  // English name.
  const link = (x: App): AppLink => ({ id: x.id, name: appName(x, locale), slug: x.slug });
  const worksWith = app.dependencies.flatMap((d) => {
    const dep = all.find((x) => x.id === d.id);
    return dep ? [link(dep)] : [];
  });

  // The strip: how mature the app is, which version, how many other apps it works with (their names
  // are the "Works with" section below it) and under which licence. The category is the quiet line in
  // the hero and the developer is in the information list; every value comes from the manifest.
  // The plural category of the count in this language; "zero" and "two" (Arabic, Welsh) fall to "other".
  const form = new Intl.PluralRules(locale).select(worksWith.length);
  const plural = form === "one" || form === "few" || form === "many" ? form : "other";
  const facts: Fact[] = [
    { label: a.meta.maturity, value: maturity },
    { label: a.meta.version, value: app.version },
    {
      label: s.worksWith,
      value:
        worksWith.length > 0
          ? fill(s.worksCount[plural], { n: String(worksWith.length) })
          : s.standalone,
    },
    { label: a.meta.license, value: meta.license },
  ].filter((f) => f.value);

  // The words on the cards: the page language's own labels where the product has them (D52), else
  // the English manifest, which the page marks with lang="en".
  const labels = loadLabels(locale, app.id);
  const translated = locale !== "en";
  const actions: Card[] = app.models.flatMap((model) =>
    model.actions.map((act): Card => {
      const own = labels.actions[`${model.name}.${act.name}`];
      const modelName = labels.models[model.name]?.name;
      const ok = translated && !!own?.label && !!own?.description && !!modelName;
      return {
        kicker: ok ? modelName : modelLabel(app, model),
        title: ok ? own!.label! : capital(words(act.name)),
        text: ok ? own!.description! : act.description,
        approval: act.approval,
        ...(translated && !ok ? { lang: "en" as const } : {}),
      };
    }),
  );
  const models: Card[] = app.models.map((model) => {
    const own = labels.models[model.name];
    const ok = translated && !!own?.name && !!own?.description;
    return {
      title: ok ? own!.name! : modelLabel(app, model),
      text: ok ? own!.description! : model.description,
      ...(translated && !ok ? { lang: "en" as const } : {}),
    };
  });

  // Three short lines at most, from the same facts as the technical "agents" section: what is
  // masked, what is withheld, and that a person approves the steps an agent can only propose.
  const fields = app.models.flatMap((model) => model.fields);
  const safe: string[] = [];
  if (fields.some((f) => agentView(f) === "masked")) safe.push(s.safeMasked);
  if (fields.some((f) => agentView(f) === "withheld")) safe.push(s.safeWithheld);
  safe.push(
    app.models.some((model) => model.actions.some((x) => x.approval))
      ? s.safeApprove
      : s.safeApproveNone,
  );

  const related = all
    .filter((x) => x.category === app.category && x.id !== app.id)
    .slice(0, 6)
    .map(link);

  const info: InfoRow[] = [
    { label: s.developer, value: developers },
    { label: s.category, value: category },
    {
      label: a.meta.version,
      value: app.builtin ? fill(s.sameAsObop, { version: app.version }) : app.version,
    },
    {
      label: a.meta.license,
      value: meta.license,
      ...(/^[A-Za-z0-9.+-]+$/.test(meta.license)
        ? { href: `https://spdx.org/licenses/${meta.license}.html` }
        : {}),
    },
    {
      label: s.compat,
      ...(plainCompat(meta.requiresCore, s.compatValue)
        ? { value: plainCompat(meta.requiresCore, s.compatValue)! }
        : { value: meta.requiresCore, mono: true }),
    },
    { label: a.meta.maturity, value: maturity },
    // A built-in app's manifest is a snapshot kept for this site; its real code is the module.
    app.builtin
      ? {
          label: s.source,
          value: app.source,
          href: `${GITHUB}/tree/main/${app.source}`,
          mono: true,
        }
      : {
          label: s.manifest,
          value: app.source,
          href: `${GITHUB}/blob/main/${app.source}`,
          mono: true,
        },
  ].filter((r) => r.value);

  const purpose = splitPurpose(app.purpose);
  return {
    category,
    facts,
    purposeLead: purpose.lead,
    purposeRest: purpose.rest,
    purposeFold: wordCount(purpose.rest) > FOLD_WORDS,
    actions,
    models,
    safe,
    worksWith,
    related,
    info,
    command: app.builtin ? null : `obop modules install-from ${app.id}`,
  };
}

import type { Messages } from "../../i18n/types.ts";
import { agentView, type App, type Model } from "./apps.ts";
import { CORE_VERSION, GITHUB } from "./paths.ts";

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
  /** The quiet "needs a person to approve" mark. */
  approval?: boolean;
}
export interface AppLink {
  id: string;
  name: string;
  slug: string;
}
export interface AppView {
  facts: Fact[];
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

const ACRONYMS: Record<string, string> = { csv: "CSV", oauth: "OAuth", sla: "SLA" };

/** `reset_to_draft` becomes "reset to draft"; the few acronyms in the manifests keep their capitals. */
export const words = (id: string): string =>
  id
    .split("_")
    .map((w) => ACRONYMS[w] ?? w)
    .join(" ");
export const capital = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);
export const lowerFirst = (s: string): string => s.charAt(0).toLowerCase() + s.slice(1);

/** A model's name without its module prefix: `helpdesk_ticket` in the Helpdesk app is a "Ticket". */
export function modelLabel(app: App, model: Model): string {
  const prefix = `${app.id}_`;
  const bare = model.name.startsWith(prefix) ? model.name.slice(prefix.length) : model.name;
  return capital(words(bare || model.name));
}

const fill = (s: string, vars: Record<string, string>) =>
  s.replace(/\{(\w+)\}/g, (_m, k: string) => vars[k] ?? "");

/** The repository path of the manifest an app page is generated from. */
export const manifestPath = (app: App): string =>
  app.builtin ? `web/apps/site/data/builtin/${app.id}.toml` : app.source;

export function appView(app: App, m: Messages, all: App[]): AppView {
  const a = m.apps;
  const s = a.store;
  const meta = app.meta;
  const category = a.categories[app.category];
  const maturity =
    a.meta.maturities[meta.maturity as keyof typeof a.meta.maturities] ?? meta.maturity;
  const developers = [meta.author, ...meta.authors].filter(Boolean).join(", ");

  const facts: Fact[] = [
    { label: a.meta.maturity, value: maturity },
    { label: s.category, value: category },
    { label: s.developer, value: meta.author },
    { label: a.meta.license, value: meta.license },
  ].filter((f) => f.value);

  const actions: Card[] = app.models.flatMap((model) =>
    model.actions.map((act): Card => ({
      kicker: modelLabel(app, model),
      title: capital(words(act.name)),
      text: act.description,
      approval: act.approval,
    })),
  );
  const models: Card[] = app.models.map((model) => ({
    title: modelLabel(app, model),
    text: model.description,
  }));

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

  const worksWith = app.dependencies.flatMap((d) => {
    const dep = all.find((x) => x.id === d.id);
    return dep ? [{ id: dep.id, name: dep.name, slug: dep.slug }] : [];
  });
  const related = all
    .filter((x) => x.category === app.category && x.id !== app.id)
    .slice(0, 6)
    .map((x) => ({ id: x.id, name: x.name, slug: x.slug }));

  const info: InfoRow[] = [
    { label: s.developer, value: developers },
    { label: s.category, value: category },
    {
      label: a.meta.version,
      value: app.builtin ? fill(a.meta.builtinVersion, { version: app.version }) : app.version,
    },
    {
      label: a.meta.license,
      value: meta.license,
      ...(/^[A-Za-z0-9.+-]+$/.test(meta.license)
        ? { href: `https://spdx.org/licenses/${meta.license}.html` }
        : {}),
    },
    {
      label: s.requires,
      value: meta.requiresCore,
      mono: true,
      note: `(${fill(a.meta.coreNow, { version: CORE_VERSION })})`,
    },
    { label: a.meta.maturity, value: maturity },
    {
      label: s.manifest,
      value: manifestPath(app),
      href: `${GITHUB}/blob/main/${manifestPath(app)}`,
      mono: true,
    },
  ].filter((r) => r.value);

  return {
    facts,
    actions,
    models,
    safe,
    worksWith,
    related,
    info,
    command: app.builtin ? null : `obop modules install-from ${app.id}`,
  };
}

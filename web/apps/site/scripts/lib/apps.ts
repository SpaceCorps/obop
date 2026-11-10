import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parse } from "smol-toml";
import { APP_COPY, BUILTIN_IDS, type CardCopy, type Category } from "../../data/apps.ts";
import { SITE_ROOT, REPO_ROOT } from "./paths.ts";

export interface Field {
  name: string;
  description: string;
  class: string;
  personal_data: boolean;
  group: string;
  kind: string;
  options?: string[];
  to?: string;
  required: boolean;
}
export interface Action {
  name: string;
  description: string;
  field?: string;
  from?: string;
  to?: string;
  group: string;
  approval: boolean;
}
export interface Model {
  name: string;
  description: string;
  read_group: string;
  write_group: string;
  fields: Field[];
  actions: Action[];
}
/** The `[module]` metadata of D55 as the manifest declares it. */
export interface Meta {
  summary: string;
  author: string;
  authors: string[];
  license: string;
  homepage: string;
  repository: string;
  documentation: string;
  support: string;
  keywords: string[];
  /** Module categories (finance, sales, supply-chain, ...), not the site's own `Category`. */
  categories: string[];
  maturity: string;
  requiresCore: string;
}
export interface Dependency {
  id: string;
  /** A semver requirement, `*` when the manifest does not say. */
  requirement: string;
}
export interface App {
  id: string;
  slug: string;
  name: string;
  category: Category;
  builtin: boolean;
  /** The module's version; a built-in module versions with the core. */
  version: string;
  description: string;
  meta: Meta;
  dependencies: Dependency[];
  tagline: string;
  purpose: string;
  /** The technical remainder of the purpose, shown under "For developers" (see `AppCopy.technical`). */
  technical?: string;
  /** Plain English names for the models that need one (see `AppCopy.modelNames`). */
  modelNames: Record<string, string>;
  /** Plain words for the cards whose manifest text is not (see `AppCopy.cards`). */
  cards: Record<string, CardCopy>;
  /** The models that have no `delete` command (see `AppCopy.noDelete`). */
  noDelete: string[];
  models: Model[];
  /** Repository path of the manifest (or the built-in snapshot). */
  source: string;
}

type Raw = Record<string, unknown>;

/** A TOML value as text when it is text, else the fallback: a missing key reads as absent. */
const str = (v: unknown, fallback = ""): string => (typeof v === "string" ? v : fallback);

function load(file: string): Raw {
  return parse(readFileSync(file, "utf8")) as Raw;
}

/** `depends = [...]` (any version) and `[module.dependencies]` (a requirement each), in that order. */
function dependenciesOf(mod: Raw): Dependency[] {
  const out: Dependency[] = ((mod["depends"] as string[] | undefined) ?? []).map((id) => ({
    id,
    requirement: "*",
  }));
  const table = (mod["dependencies"] as Record<string, string> | undefined) ?? {};
  for (const id of Object.keys(table).sort()) out.push({ id, requirement: String(table[id]) });
  return out;
}

function toApp(raw: Raw, builtin: boolean, source: string): App {
  const mod = raw["module"] as Raw;
  const id = String(mod["id"]);
  const copy = APP_COPY.find((a) => a.id === id);
  if (!copy) throw new Error(`no copy in data/apps.ts for app ${id}`);
  const models = ((raw["model"] as Raw[] | undefined) ?? []).map((m): Model => {
    const rg = String(m["read_group"]);
    return {
      name: String(m["name"]),
      // The manifest's words, unless data/apps.ts corrects one that says more than the module does.
      description: copy.modelText?.[String(m["name"])] ?? String(m["description"]),
      read_group: rg,
      write_group: String(m["write_group"]),
      fields: ((m["field"] as Raw[] | undefined) ?? []).map((f): Field => ({
        name: String(f["name"]),
        // The manifest's words, unless data/apps.ts corrects one that says more than the module does.
        description:
          copy.fieldText?.[`${String(m["name"])}.${String(f["name"])}`] ?? String(f["description"]),
        class: String(f["class"]),
        personal_data: f["personal_data"] === true,
        group: str(f["group"], rg),
        kind: str(f["kind"], "text"),
        options: f["options"] as string[] | undefined,
        to: f["to"] as string | undefined,
        required: f["required"] === true,
      })),
      actions: ((m["action"] as Raw[] | undefined) ?? []).map((a): Action => ({
        name: String(a["name"]),
        description: String(a["description"]),
        field: a["field"] as string | undefined,
        from: a["from"] as string | undefined,
        to: a["to"] as string | undefined,
        group: str(a["group"], String(m["write_group"])),
        approval: a["approval"] === true,
      })),
    };
  });
  return {
    id,
    slug:
      copy.id === "base" ? "contacts" : copy.id === "product" ? "products" : id.replace(/_/g, "-"),
    name: copy.name,
    category: copy.category,
    builtin,
    version: String(mod["version"]),
    description: copy.packageText ?? String(mod["description"]),
    meta: {
      summary: str(mod["summary"]),
      author: str(mod["author"]),
      authors: (mod["authors"] as string[] | undefined) ?? [],
      license: str(mod["license"]),
      homepage: str(mod["homepage"]),
      repository: str(mod["repository"]),
      documentation: str(mod["documentation"]),
      support: str(mod["support"]),
      keywords: (mod["keywords"] as string[] | undefined) ?? [],
      categories: (mod["categories"] as string[] | undefined) ?? [],
      maturity: str(mod["maturity"]),
      requiresCore: str(mod["requires_core"], "*"),
    },
    dependencies: dependenciesOf(mod),
    tagline: copy.tagline,
    purpose: copy.purpose,
    ...(copy.technical ? { technical: copy.technical } : {}),
    modelNames: copy.modelNames ?? {},
    cards: copy.cards ?? {},
    noDelete: copy.noDelete ?? [],
    models,
    source,
  };
}

/** All 28 apps in display order: the six built in first, then the bundled marketplace packages. */
export function loadApps(): App[] {
  const apps: App[] = [];
  for (const id of BUILTIN_IDS) {
    apps.push(
      toApp(load(join(SITE_ROOT, "data/builtin", `${id}.toml`)), true, `crates/modules/${id}`),
    );
  }
  const bundled = new Set(APP_COPY.map((a) => a.id));
  for (const id of BUILTIN_IDS) bundled.delete(id);
  for (const copy of APP_COPY) {
    if (!bundled.has(copy.id)) continue;
    apps.push(
      toApp(
        load(join(REPO_ROOT, "packages", `${copy.id}.toml`)),
        false,
        `packages/${copy.id}.toml`,
      ),
    );
  }
  // Display order = APP_COPY order.
  const order = new Map(APP_COPY.map((a, i) => [a.id, i]));
  apps.sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0));
  return apps;
}

/**
 * The translated words of one app in one language, from `packages/labels/<locale>/<id>.toml`
 * (D52): the name and description of each model, the label and description of each action. These
 * are the texts the product itself shows in that language, so the cards of a translated app page
 * can use them instead of the English manifest. Empty for English and for a missing file.
 */
export interface Labels {
  /** The module's own name in that language (`[labels.<locale>.module] name`). */
  moduleName?: string;
  models: Record<string, { name?: string; description?: string }>;
  actions: Record<string, { label?: string; description?: string }>;
}
const labelCache = new Map<string, Labels>();
export function loadLabels(locale: string, id: string): Labels {
  const key = `${locale}/${id}`;
  const hit = labelCache.get(key);
  if (hit) return hit;
  const out: Labels = { models: {}, actions: {} };
  const file = join(REPO_ROOT, "packages", "labels", locale, `${id}.toml`);
  // English has no label file; a missing file means the cards fall back to the English manifest.
  if (locale !== "en" && existsSync(file)) {
    const doc = load(file);
    const own = (doc["labels"] as Raw | undefined)?.[locale] as Raw | undefined;
    const name = (own?.["module"] as Raw | undefined)?.["name"];
    if (typeof name === "string" && name.trim()) out.moduleName = name.trim();
    const models = own?.["model"] ?? {};
    for (const [model, raw] of Object.entries(models as Record<string, Raw>)) {
      out.models[model] = {
        name: raw["name"] as string | undefined,
        description: raw["description"] as string | undefined,
      };
      for (const [action, a] of Object.entries((raw["action"] as Record<string, Raw>) ?? {}))
        out.actions[`${model}.${action}`] = {
          label: a["label"] as string | undefined,
          description: a["description"] as string | undefined,
        };
    }
  }
  labelCache.set(key, out);
  return out;
}

/**
 * The name a page in `locale` calls the app: the product's own name for the module in that language
 * (`packages/labels`, D52, so "Fakturering" for Invoicing in Swedish, as in the product), else the
 * English name from `data/apps.ts`.
 */
export function appName(app: App, locale: string): string {
  return loadLabels(locale, app.id).moduleName ?? app.name;
}

export type AgentView = "clear" | "masked" | "withheld";

/**
 * The data classes an agent token never receives (obop-masking `mode_for`: a sensitive class or a
 * secret is denied; every other class is allowed, personal data masked). A money field of class
 * `none` is not among them: an AI agent sees an order total.
 */
export const WITHHELD_CLASSES = [
  "financial",
  "confidential_hr",
  "special_category",
  "secret",
] as const;
export type WithheldClass = (typeof WITHHELD_CLASSES)[number];

/** What an agent token sees of a field (D40): sensitive classes are withheld, personal data masked. */
export function agentView(f: Field): AgentView {
  if ((WITHHELD_CLASSES as readonly string[]).includes(f.class)) return "withheld";
  if (f.personal_data) return "masked";
  return "clear";
}

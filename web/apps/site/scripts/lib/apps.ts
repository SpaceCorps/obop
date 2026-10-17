import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parse } from "smol-toml";
import { APP_COPY, BUILTIN_IDS, type Category } from "../../data/apps.ts";
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
  models: Model[];
  /** Repository path of the manifest (or the built-in snapshot). */
  source: string;
}

type Raw = Record<string, unknown>;

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
      description: String(m["description"]),
      read_group: rg,
      write_group: String(m["write_group"]),
      fields: ((m["field"] as Raw[] | undefined) ?? []).map((f): Field => ({
        name: String(f["name"]),
        description: String(f["description"]),
        class: String(f["class"]),
        personal_data: f["personal_data"] === true,
        group: String(f["group"] ?? rg),
        kind: String(f["kind"] ?? "text"),
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
        group: String(a["group"] ?? m["write_group"]),
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
    description: String(mod["description"]),
    meta: {
      summary: String(mod["summary"] ?? ""),
      author: String(mod["author"] ?? ""),
      authors: (mod["authors"] as string[] | undefined) ?? [],
      license: String(mod["license"] ?? ""),
      homepage: String(mod["homepage"] ?? ""),
      repository: String(mod["repository"] ?? ""),
      documentation: String(mod["documentation"] ?? ""),
      support: String(mod["support"] ?? ""),
      keywords: (mod["keywords"] as string[] | undefined) ?? [],
      categories: (mod["categories"] as string[] | undefined) ?? [],
      maturity: String(mod["maturity"] ?? ""),
      requiresCore: String(mod["requires_core"] ?? "*"),
    },
    dependencies: dependenciesOf(mod),
    tagline: copy.tagline,
    purpose: copy.purpose,
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

export type AgentView = "clear" | "masked" | "withheld";

/** What an agent token sees of a field (D40): sensitive classes are withheld, personal data masked. */
export function agentView(f: Field): AgentView {
  if (["confidential_hr", "special_category", "financial", "secret"].includes(f.class))
    return "withheld";
  if (f.personal_data) return "masked";
  return "clear";
}

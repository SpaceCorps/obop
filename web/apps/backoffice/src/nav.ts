// Where an app opens and how its sidebar reads: the home of a module, its menu groups, the plural
// names of its records and the category it is listed under on the home screen. Pure functions over
// `/api/meta`; the decisions themselves are presentation.ts's (`homeOf`, `menuOf`), which read the
// D95 hints first and fall back to heuristics.
import { getLang, isPseudo, labelFor, msg } from "@obop/ui";
import type { ModelMeta, ModuleMeta } from "./api.ts";
import { modelTitle } from "./labels.ts";
import { homeOf, indexModels, menuOf, type ModuleMenu } from "./presentation.ts";
import { OVERVIEW, href, overviewHref } from "./route.ts";

/** What these functions need of an app (Chrome.tsx `App`). */
type AppLike = { id: string; module: ModuleMeta };

const menus = new WeakMap<ModuleMeta, ModuleMenu>();

/**
 * The sidebar groups of a module: main, configuration, reporting and hidden (child models, reached
 * through their parent). Kept per metadata object, so a render asks once.
 */
export function appMenu(module: ModuleMeta): ModuleMenu {
  let m = menus.get(module);
  if (!m) {
    m = menuOf(module, indexModels([module]));
    menus.set(module, m);
  }
  return m;
}

/** Where a module opens: a model name, or `overview` (OVERVIEW). Empty for a module without models. */
export function appHome(module: ModuleMeta): string {
  if (module.home) return module.home;
  const menu = appMenu(module);
  // The same rule as presentation.ts `homeOf`, on the cached menu.
  if (menu.main.length >= 2) return OVERVIEW;
  return menu.main[0]?.name ?? homeOf(module, indexModels([module]));
}

/** Whether the app has a generated overview page (two or more main models, or the hint says so). */
export function hasOverview(module: ModuleMeta): boolean {
  return module.home === OVERVIEW || (!module.home && appMenu(module).main.length >= 2);
}

/** The address of an app's home. */
export function homeHref(app: AppLike): string {
  const home = appHome(app.module);
  return home === OVERVIEW ? overviewHref(app.id) : href({ app: app.id, model: home });
}

/** The model a route without a model shows: the home model, else the first model. */
export function homeModel(module: ModuleMeta): ModelMeta | undefined {
  const home = appHome(module);
  return module.models.find((m) => m.name === home) ?? appMenu(module).main[0] ?? module.models[0];
}

// ---- plural names --------------------------------------------------------------------------------

const IRREGULAR: Record<string, string> = { person: "people", child: "children", man: "men" };
const UNCHANGED = /(series|news|species|data|staff|equipment|information|feedback)$/i;

/** The English plural of a label, by its last word: "Entry" -> "Entries", "Tax" -> "Taxes". */
export function englishPlural(label: string): string {
  const m = /^(.*?)(\p{L}+)(\W*)$/u.exec(label);
  if (!m) return label;
  const [, head = "", word = "", tail = ""] = m;
  const lower = word.toLowerCase();
  let out: string;
  if (IRREGULAR[lower]) {
    const p = IRREGULAR[lower]!;
    out = word[0] === word[0]!.toUpperCase() ? p.charAt(0).toUpperCase() + p.slice(1) : p;
  } else if (UNCHANGED.test(word) || /(ings|ics)$/i.test(word)) out = word;
  else if (/[^aeiou]y$/i.test(word)) out = `${word.slice(0, -1)}ies`;
  else if (/(s|x|z|ch|sh)$/i.test(word)) out = `${word}es`;
  else out = `${word}s`;
  return `${head}${out}${tail}`;
}

/**
 * The plural name of a model for the sidebar, the palette and list headings: the server's
 * `plural_labels` in the active language, else (in English) `plural`, else the singular label made
 * plural by the English rule. In any other language the singular translated label stays: a rule
 * for one language would make words up in another.
 */
export function pluralTitle(
  m: Pick<ModelMeta, "name" | "labels" | "plural" | "plural_labels">,
  moduleId: string,
): string {
  const lang = getLang();
  const own = labelFor({ labels: m.plural_labels }, lang, "");
  if (own) return own;
  const english = lang.split("-")[0] === "en" || isPseudo(lang);
  if (english && m.plural) return m.plural;
  const one = modelTitle(m, moduleId);
  return english ? englishPlural(one) : one;
}

// ---- categories of the home screen ---------------------------------------------------------------

/**
 * The groups of the home screen, in order, and the D55 categories each takes. An app goes to the
 * group of its first category other than `platform` (Contacts is `platform, sales`: Sales); an app
 * with only `platform` goes to Settings, one without categories to More apps.
 */
export const APP_GROUPS = [
  { id: "sales", label: msg("Sales"), from: ["sales"] },
  { id: "finance", label: msg("Finance"), from: ["finance"] },
  { id: "operations", label: msg("Operations"), from: ["supply-chain", "services"] },
  { id: "website", label: msg("Website and marketing"), from: ["marketing"] },
  { id: "people", label: msg("People"), from: ["people"] },
  { id: "productivity", label: msg("Productivity"), from: ["productivity"] },
  { id: "settings", label: msg("Settings"), from: ["platform"] },
  { id: "other", label: msg("More apps"), from: ["other"] },
] as const;

export type AppGroupId = (typeof APP_GROUPS)[number]["id"];

/** The home screen group of an app. */
export function appGroup(app: AppLike): AppGroupId {
  if (app.id === "customize") return "settings";
  const cats = app.module.categories ?? [];
  const first = cats.find((c) => c !== "platform") ?? cats[0];
  if (!first) return "other";
  return APP_GROUPS.find((g) => (g.from as readonly string[]).includes(first))?.id ?? "other";
}

/** The apps in their home screen groups, each group in the order of `apps`; empty groups left out. */
export function groupApps<A extends AppLike>(
  apps: readonly A[],
): { id: AppGroupId; label: string; apps: A[] }[] {
  const out = APP_GROUPS.map((g) => ({ id: g.id, label: g.label as string, apps: [] as A[] }));
  for (const a of apps) out.find((g) => g.id === appGroup(a))!.apps.push(a);
  return out.filter((g) => g.apps.length > 0);
}

/** Whether the app has a tile on the launcher and the home screen (D95 `launcher = false` hides it). */
export const onLauncher = (app: AppLike): boolean => app.module.launcher !== false;

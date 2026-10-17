import { href as routeHref } from "../route.ts";
import type { ModulesRoute, PanelTab, Tab } from "./types.ts";

export const TAB_IDS: Tab[] = ["installed", "marketplace", "updates"];
export const PANEL_IDS: PanelTab[] = ["overview", "dependencies", "contents", "versions"];

/** The Modules page's place in the address: `?view=marketplace&id=timesheets&section=dependencies`. */
export function parseModulesRoute(r: { view: string; id: string; section: string }): ModulesRoute {
  return {
    tab: (TAB_IDS as string[]).includes(r.view) ? (r.view as Tab) : "installed",
    id: r.id,
    panel: (PANEL_IDS as string[]).includes(r.section) ? (r.section as PanelTab) : "overview",
  };
}

export function modulesHref(r: Partial<ModulesRoute>): string {
  return routeHref({
    app: "customize",
    model: "modules",
    tab: "modules",
    view: r.tab && r.tab !== "installed" ? r.tab : "",
    id: r.id ?? "",
    section: r.panel && r.panel !== "overview" ? r.panel : "",
  });
}

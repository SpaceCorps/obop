import { useCallback, useMemo, useState } from "react";
import { describeFor, getLang, t } from "@obop/ui";
import { getCoreVersion, type ModuleMeta } from "../api.ts";
import { appTitle } from "../labels.ts";
import { useModules, type Loaded } from "./data.ts";
import type { CardActions } from "./ModuleCard.tsx";
import type { PlanRequest } from "./PlanDialog.tsx";
import { modulesHref } from "./route.ts";
import type { ModulesRoute, Row } from "./types.ts";

export type Dialog =
  | { kind: "plan"; request: PlanRequest; title: string; name: string; verb: "install" | "update" }
  | { kind: "uninstall"; row: Row }
  | { kind: "rollback"; row: Row }
  | { kind: "updateall" }
  | null;

/** A one-line card text: the summary, else the first sentence of the description. */
export function cardText(r: Row, description: string, summary?: string): string {
  if (summary || r.summary) return summary || r.summary;
  const first = description.split(/(?<=[.!?])\s/)[0] ?? description;
  return first.length > 140 ? `${first.slice(0, 137)}…` : first;
}

export type View = ReturnType<typeof useModulesView>;

/**
 * Everything the Modules page and its side panel share: the loaded rows, the names as shown in the
 * active language, the dependency index, the selected module, and the actions that open the plan
 * and uninstall dialogs. One instance lives in Customize so the card list and the floating panel
 * stay in step.
 */
export function useModulesView(opts: {
  token: string;
  enabled: boolean;
  meta: ModuleMeta[];
  route: ModulesRoute;
  go: (r: Partial<ModulesRoute>) => void;
  notify: (text: string, tone?: "ok" | "error" | "info") => void;
  onChanged: () => void;
}) {
  const { token, enabled, meta, route, go, notify, onChanged } = opts;
  const data: Loaded = useModules(token, enabled);
  const [dialog, setDialog] = useState<Dialog>(null);
  const core = getCoreVersion() || "";

  // Installed modules bring their own names and descriptions per language (D52); a package that is
  // only offered is shown from the English listing.
  const rowsById = useMemo(() => {
    const m = new Map<string, Row>();
    for (const r of data.market) m.set(r.id, r);
    for (const r of data.installed) m.set(r.id, r);
    return m;
  }, [data.installed, data.market]);
  const nameOf = useCallback(
    (id: string) => {
      const m = meta.find((x) => x.id === id);
      return m ? appTitle(m) : (rowsById.get(id)?.name ?? id);
    },
    [meta, rowsById],
  );
  const descriptionOf = useCallback(
    (r: Row) => {
      const m = meta.find((x) => x.id === r.id);
      return (m && describeFor(m)) || r.description;
    },
    [meta],
  );

  /** The card summary in the active language: the module's translation, else the English one. */
  const summaryOf = useCallback(
    (r: Row) => {
      const m = meta.find((x) => x.id === r.id);
      const hit = m?.summaries?.[getLang()] ?? m?.summaries?.[getLang().split("-")[0]!];
      return (typeof hit === "string" && hit.trim() ? hit : "") || r.summary;
    },
    [meta],
  );

  const installedIds = useMemo(() => new Set(data.installed.map((r) => r.id)), [data.installed]);
  // The selected module: from the list the address names; an installed module's facts (contents,
  // who installed it) are added to its marketplace row so the panel shows everything known.
  const selected = useMemo(() => {
    if (!route.id) return null;
    const inst = data.installed.find((r) => r.id === route.id);
    const mkt = data.market.find((r) => r.id === route.id);
    if (route.tab === "marketplace" && mkt) {
      return {
        kind: "market" as const,
        row: inst
          ? {
              ...mkt,
              contents: inst.contents,
              installed_at: inst.installed_at,
              installed_by: inst.installed_by,
              dependents: inst.dependents,
              warnings: inst.warnings,
            }
          : mkt,
      };
    }
    if (inst) return { kind: "installed" as const, row: inst };
    if (mkt) return { kind: "market" as const, row: mkt };
    return null;
  }, [route.id, route.tab, data.installed, data.market]);

  const actions: CardActions = useMemo(
    () => ({
      onInstall: (r) =>
        setDialog({
          kind: "plan",
          request: { id: r.id },
          title: t("Install {name}", { name: nameOf(r.id) }),
          name: nameOf(r.id),
          verb: "install",
        }),
      onUpdate: (r) =>
        setDialog({
          kind: "plan",
          request: { id: r.id },
          title: t("Update {name}", { name: nameOf(r.id) }),
          name: nameOf(r.id),
          verb: "update",
        }),
      onUninstall: (r) => setDialog({ kind: "uninstall", row: r }),
      onRollback: (r) => setDialog({ kind: "rollback", row: r }),
      onUpdateAll: () => setDialog({ kind: "updateall" }),
    }),
    [nameOf],
  );

  const hrefFor = useCallback(
    (id: string) =>
      rowsById.has(id) ? modulesHref({ tab: route.tab, id, panel: route.panel }) : null,
    [rowsById, route.tab, route.panel],
  );

  const finished = useCallback(
    async (text: string, tone: "ok" | "info" = "ok") => {
      // The toast comes after the lists have caught up, so what it says is what the page shows.
      await data.reload();
      setDialog(null);
      notify(text, tone);
      onChanged();
    },
    [data, notify, onChanged],
  );

  return {
    data,
    core,
    nameOf,
    descriptionOf,
    summaryOf,
    rowsById,
    installedIds,
    selected,
    actions,
    hrefFor,
    dialog,
    setDialog,
    finished,
    route,
    go,
    notify,
    token,
  };
}

import { useMemo, useState } from "react";
import { Badge, Button, EmptyState, Icon, Notice, Select, Tabs, panelId, t } from "@obop/ui";
import { ModuleCard } from "./ModuleCard.tsx";
import { DependencyOverview } from "./DependencyOverview.tsx";
import { PlanDialog, UninstallDialog } from "./PlanDialog.tsx";
import { UploadArea } from "./UploadArea.tsx";
import {
  categoriesOf,
  categoryLabel,
  filterRows,
  sortRows,
  withUpdates,
  type SortKey,
} from "./status.ts";
import { modulesHref } from "./route.ts";
import { cardText, type View } from "./view.ts";
import type { Row, Tab } from "./types.ts";

/**
 * Customize > Modules: the core version and what it means, Installed / Marketplace / Updates,
 * search, category and sort, one card per module, the dependency overview of the instance, and
 * the upload area. The detail panel lives beside this section (see Customize) and is driven by
 * the same `View`.
 */
export function ModulesPage({
  view,
  guideHref,
  goBuild,
}: {
  view: View;
  guideHref: string;
  goBuild: () => void;
}) {
  const { data, route, core } = view;
  const [q, setQ] = useState("");
  const [category, setCategory] = useState("");
  const [sort, setSort] = useState<SortKey>("name");
  const tab = route.tab;
  const updates = useMemo(() => withUpdates(data.installed), [data.installed]);
  const source: Row[] =
    tab === "installed" ? data.installed : tab === "marketplace" ? data.market : updates;
  const nameOfRow = (r: Row) => view.nameOf(r.id);
  const shown = sortRows(filterRows(source, q, category, nameOfRow), sort, nameOfRow);
  const cats = useMemo(
    () => categoriesOf([...data.installed, ...data.market]),
    [data.installed, data.market],
  );
  const filtering = q.trim() !== "" || category !== "";
  const cardKind = tab === "marketplace" ? "market" : "installed";

  return (
    <div className="stack gap-lg mods">
      <header className="mods-head">
        <div className="mods-head-text">
          <h2>{t("Modules")}</h2>
          <p className="hint">
            {t(
              "Core {version} is the platform every module runs on. A module says which core versions it works with and which other modules it needs, so you see what an install brings along before you confirm it.",
              { version: core || "?" },
            )}{" "}
            <a className="reflink" href={guideHref}>
              {t("How to write a custom module?")}
            </a>
          </p>
        </div>
        <Badge
          tone="accent"
          dot={false}
          className="mods-core"
          title={t("The running core version")}
        >
          <Icon name="shield-check" size={14} />
          {t("Core {version}", { version: core || "?" })}
        </Badge>
      </header>

      {data.error && (
        <Notice
          tone="danger"
          title={t("Some module information could not be loaded")}
          action={
            <Button size="sm" onClick={() => void data.reload()}>
              {t("Try again")}
            </Button>
          }
        >
          {data.error}
        </Notice>
      )}

      <Tabs
        label={t("Modules")}
        idPrefix="mods"
        value={tab}
        onChange={(id: Tab) => view.go({ tab: id, id: "" })}
        tabs={[
          { id: "installed", label: t("Installed"), count: data.installed.length },
          { id: "marketplace", label: t("Marketplace"), count: data.market.length },
          { id: "updates", label: t("Updates"), count: updates.length },
        ]}
      />

      <div
        className="stack gap-lg"
        role="tabpanel"
        id={panelId("mods")}
        aria-labelledby={`mods-tab-${tab}`}
      >
        <div className="mods-toolbar" role="search" aria-label={t("Find a module")}>
          <div className="searchfield mods-search">
            <Icon name="search" size={16} />
            <input
              placeholder={t("Search modules")}
              aria-label={t("Search modules")}
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <Select
            label={t("Category")}
            value={category}
            onChange={(e) => setCategory(e.target.value)}
          >
            <option value="">{t("All categories")}</option>
            {cats.map((c) => (
              <option key={c} value={c}>
                {categoryLabel(c)}
              </option>
            ))}
          </Select>
          <Select
            label={t("Sort by")}
            value={sort}
            onChange={(e) => setSort(e.target.value as SortKey)}
          >
            <option value="name">{t("Name")}</option>
            <option value="recent">{t("Recently installed")}</option>
            <option value="needed">{t("Most needed by others")}</option>
          </Select>
        </div>

        {data.loading ? (
          <ul className="mod-grid" aria-busy="true" aria-label={t("Loading modules")}>
            {Array.from({ length: 6 }, (_, i) => (
              <li key={i}>
                <div className="card mod-card skeleton-card" aria-hidden="true">
                  <span className="skeleton-line wide" />
                  <span className="skeleton-line" />
                  <span className="skeleton-line short" />
                </div>
              </li>
            ))}
          </ul>
        ) : shown.length === 0 ? (
          <EmptyState
            icon="plug"
            title={
              filtering
                ? t("No module matches")
                : tab === "updates"
                  ? t("Everything is up to date")
                  : tab === "marketplace"
                    ? t("The marketplace has nothing to offer")
                    : t("No modules are installed")
            }
            hint={
              filtering
                ? t("Try another word or clear the filters.")
                : tab === "updates"
                  ? t(
                      "When the marketplace offers a newer version of an installed module, it appears here.",
                    )
                  : undefined
            }
            action={
              filtering
                ? {
                    label: t("Clear filters"),
                    onClick: () => {
                      setQ("");
                      setCategory("");
                    },
                  }
                : undefined
            }
          />
        ) : (
          <ul className="mod-grid" aria-label={t("Modules")}>
            {shown.map((r) => (
              <li key={`${tab}-${r.id}`}>
                <ModuleCard
                  row={r}
                  kind={cardKind}
                  title={view.nameOf(r.id)}
                  summary={cardText(r, view.descriptionOf(r), view.summaryOf(r))}
                  core={core}
                  nameOf={view.nameOf}
                  href={modulesHref({ tab, id: r.id })}
                  selected={route.id === r.id}
                  actions={view.actions}
                />
              </li>
            ))}
          </ul>
        )}

        {tab === "installed" && !data.loading && data.installed.length > 0 && (
          <section className="stack" aria-labelledby="mods-overview-h">
            <h3 id="mods-overview-h" className="overline">
              {t("How your modules fit together")}
            </h3>
            <DependencyOverview
              rows={data.installed}
              nameOf={view.nameOf}
              hrefFor={(id) => modulesHref({ tab: "installed", id, panel: "dependencies" })}
            />
          </section>
        )}

        {tab === "marketplace" && (
          <UploadArea
            token={view.token}
            nameOf={view.nameOf}
            onBuild={goBuild}
            onInstall={(manifest) =>
              view.setDialog({
                kind: "plan",
                request: { manifest },
                title: t("Install this package"),
                name: "",
                verb: "install",
              })
            }
          />
        )}
      </div>

      {view.dialog?.kind === "plan" && (
        <PlanDialog
          token={view.token}
          request={view.dialog.request}
          title={view.dialog.title}
          nameOf={view.nameOf}
          rows={[...data.installed, ...data.market]}
          onClose={() => view.setDialog(null)}
          onDone={(o) => {
            const name = o.rootName || (view.dialog?.kind === "plan" ? view.dialog.name : "");
            const verb = view.dialog?.kind === "plan" ? view.dialog.verb : "install";
            void view.finished(
              o.proposed
                ? t("Waiting for a human to approve")
                : verb === "update"
                  ? t("Updated {name}", { name })
                  : t("Installed {name}", { name }),
              o.proposed ? "info" : "ok",
            );
          }}
        />
      )}
      {view.dialog?.kind === "uninstall" && (
        <UninstallDialog
          token={view.token}
          row={view.dialog.row}
          title={view.nameOf(view.dialog.row.id)}
          nameOf={view.nameOf}
          onClose={() => view.setDialog(null)}
          onDone={() => {
            const name = view.dialog?.kind === "uninstall" ? view.nameOf(view.dialog.row.id) : "";
            if (route.id === (view.dialog?.kind === "uninstall" ? view.dialog.row.id : ""))
              view.go({ id: "" });
            void view.finished(t("Removed {name}", { name }));
          }}
        />
      )}
    </div>
  );
}

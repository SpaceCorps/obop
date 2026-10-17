import { Button, t } from "@obop/ui";
import { AppIcon } from "../Chrome.tsx";
import { appColor } from "../theme.ts";
import { CoreBadge, LicenseBadge, MaturityBadge, SourceBadge } from "./badges.tsx";
import { dependencyLine, installState } from "./status.ts";
import type { Row } from "./types.ts";

export type CardActions = {
  onInstall: (r: Row) => void;
  onUpdate: (r: Row) => void;
  onUninstall: (r: Row) => void;
};

/** The buttons of a module, shared by the card and the detail panel. */
export function ModuleButtons({
  row,
  kind,
  actions,
}: {
  row: Row;
  /** `installed` rows come from the installed list, `market` rows from the marketplace. */
  kind: "installed" | "market";
  actions: CardActions;
}) {
  if (kind === "installed") {
    if (row.source === "builtin") return <span className="hint">{t("Part of the core")}</span>;
    return (
      <>
        {row.update_available && (
          <Button variant="primary" size="sm" onClick={() => actions.onUpdate(row)}>
            {t("Update to {version}", { version: row.update_available })}
          </Button>
        )}
        <Button size="sm" onClick={() => actions.onUninstall(row)}>
          {t("Uninstall")}
        </Button>
      </>
    );
  }
  const state = installState(row);
  if (state === "installed")
    return (
      <Button size="sm" icon="check" disabled>
        {t("Installed")}
      </Button>
    );
  if (state === "update")
    return (
      <Button variant="primary" size="sm" onClick={() => actions.onUpdate(row)}>
        {t("Update to {version}", { version: row.update_available ?? "" })}
      </Button>
    );
  // Blocked and incompatible modules open the same dialog, which says what stops them.
  return (
    <Button variant="primary" size="sm" onClick={() => actions.onInstall(row)}>
      {t("Install")}
    </Button>
  );
}

/**
 * One module as a card: icon, name, summary, version, author, licence, maturity, whether it works
 * with the running core, where it comes from and how it relates to other modules. The name is the
 * link to the detail panel and stretches over the whole card; the buttons sit above it.
 */
export function ModuleCard({
  row,
  kind,
  title,
  summary,
  core,
  nameOf,
  href,
  selected,
  actions,
}: {
  row: Row;
  kind: "installed" | "market";
  title: string;
  summary: string;
  core: string;
  nameOf: (id: string) => string;
  href: string;
  selected: boolean;
  actions: CardActions;
}) {
  const line = dependencyLine(row, nameOf);
  const version = kind === "market" && row.installed_version ? row.installed_version : row.version;
  return (
    <article className={`card mod-card${selected ? " selected" : ""}`} aria-label={title}>
      <div className="mod-top">
        <AppIcon
          app={{ id: row.id, name: row.name, color: appColor(row.id, undefined, row.name) }}
          size={44}
        />
        <div className="mod-title">
          <h3>
            <a
              className="mod-link"
              href={href}
              data-module={row.id}
              aria-current={selected ? "true" : undefined}
            >
              {title}
            </a>
          </h3>
          <span className="hint tabular">
            {version}
            {row.author ? ` · ${row.author}` : ""}
          </span>
        </div>
      </div>
      <p className="mod-summary">{summary}</p>
      <div className="mod-badges">
        <LicenseBadge row={row} />
        <MaturityBadge maturity={row.maturity} />
        <CoreBadge row={row} core={core} />
        <SourceBadge source={row.source} />
      </div>
      <p className="hint mod-deps">{line || t("Needs no other module.")}</p>
      <div className="mod-actions">
        <ModuleButtons row={row} kind={kind} actions={actions} />
      </div>
    </article>
  );
}

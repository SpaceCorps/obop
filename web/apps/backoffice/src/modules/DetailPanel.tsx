import { useEffect, useRef, type ReactNode } from "react";
import { Badge, Button, Chip, IconButton, Icon, Notice, Tabs, fmt, panelId, t } from "@obop/ui";
import { AppIcon } from "../Chrome.tsx";
import { appColor } from "../theme.ts";
import { CoreBadge, LicenseBadge, MaturityBadge, SourceBadge } from "./badges.tsx";
import { CoreRequirement, DependencyTree } from "./DependencyTree.tsx";
import { ModuleButtons, type CardActions } from "./ModuleCard.tsx";
import {
  buildTree,
  categoryLabel,
  dependencyStatus,
  flattenTree,
  prettyReq,
  sourceLabel,
} from "./status.ts";
import { ChangeList, Notes } from "./UpgradeDetails.tsx";
import type { Contents, PanelTab, Row } from "./types.ts";

const TABS: { id: PanelTab; label: () => string }[] = [
  { id: "overview", label: () => t("Overview") },
  { id: "dependencies", label: () => t("Dependencies") },
  { id: "contents", label: () => t("Contents") },
  { id: "versions", label: () => t("Versions") },
];

function Meta({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="meta-row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer" className="ext-link">
      {children}
      <Icon name="external" size={12} />
      <span className="sr-only"> {t("(opens in a new tab)")}</span>
    </a>
  );
}

/**
 * The floating side panel of one module: Overview (what it is and the metadata), Dependencies
 * (the core requirement, the tree of what it needs with required range, installed version and
 * status, and who needs it), Contents (what it declares) and Versions.
 */
export function DetailPanel({
  row,
  kind,
  panel,
  setPanel,
  onClose,
  core,
  title,
  description,
  nameOf,
  index,
  hrefFor,
  actions,
}: {
  row: Row;
  kind: "installed" | "market";
  panel: PanelTab;
  setPanel: (p: PanelTab) => void;
  onClose: () => void;
  core: string;
  title: string;
  description: string;
  nameOf: (id: string) => string;
  /** Modules whose dependencies are known: installed ones win over marketplace listings. */
  index: Map<string, Row>;
  hrefFor: (id: string) => string | null;
  actions: CardActions;
}) {
  const ref = useRef<HTMLElement>(null);
  // Focus the panel while it is open and give focus back to the card that opened it.
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    ref.current?.focus({ preventScroll: true });
    return () => {
      const back = document.querySelector<HTMLElement>(
        `[data-module="${opener?.dataset.module ?? ""}"]`,
      );
      if (back) back.focus({ preventScroll: true });
      else if (opener && document.contains(opener)) opener.focus({ preventScroll: true });
    };
  }, []);
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".overlay"))
        onClose();
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, [onClose]);

  return (
    <aside
      ref={ref}
      className="record panel mod-panel"
      aria-label={t("{name} module", { name: title })}
      tabIndex={-1}
    >
      <header>
        <div className="mod-panel-title">
          <AppIcon
            app={{ id: row.id, name: row.name, color: appColor(row.id, undefined, row.name) }}
            size={40}
          />
          <div>
            <h2>{title}</h2>
            <span className="hint tabular">
              {t("Version {version}", { version: row.version })}
              {row.author ? ` · ${row.author}` : ""}
            </span>
          </div>
        </div>
        <IconButton icon="close" label={t("Close module")} onClick={onClose} />
      </header>
      <div className="mod-panel-top">
        <div className="mod-badges">
          <LicenseBadge row={row} />
          <MaturityBadge maturity={row.maturity} />
          <CoreBadge row={row} core={core} />
          <SourceBadge source={row.source} />
        </div>
        <div className="mod-actions">
          <ModuleButtons row={row} kind={kind} actions={actions} />
        </div>
      </div>
      <Tabs
        label={t("About this module")}
        idPrefix="modp"
        value={panel}
        onChange={setPanel}
        tabs={TABS.map((x) => ({ id: x.id, label: x.label() }))}
      />
      <div
        className="mod-panel-body"
        role="tabpanel"
        id={panelId("modp")}
        aria-labelledby={`modp-tab-${panel}`}
      >
        {panel === "overview" && <Overview row={row} core={core} description={description} />}
        {panel === "dependencies" && (
          <Dependencies
            row={row}
            core={core}
            title={title}
            nameOf={nameOf}
            index={index}
            hrefFor={hrefFor}
          />
        )}
        {panel === "contents" && <ContentsTab row={row} kind={kind} />}
        {panel === "versions" && <Versions row={row} kind={kind} actions={actions} />}
      </div>
    </aside>
  );
}

function installedAt(r: Row): string {
  return r.installed_at
    ? fmt.dateOf(new Date(r.installed_at * 1000), { dateStyle: "medium", timeStyle: "short" })
    : "";
}

function Overview({ row, core, description }: { row: Row; core: string; description: string }) {
  const authors = row.authors.length ? row.authors : row.author ? [row.author] : [];
  return (
    <div className="stack">
      {!row.compatible && (
        <Notice tone="danger" title={t("Does not work with this core")}>
          {t("It needs core {requirement}, and you run {version}. It cannot be installed here.", {
            requirement: prettyReq(row.requires_core),
            version: core,
          })}
        </Notice>
      )}
      <p className="mod-desc">{description || row.summary}</p>
      {(row.warnings ?? []).length > 0 && (
        <Notice tone="warning" title={t("Worth knowing")}>
          {(row.warnings ?? []).join(" ")}
        </Notice>
      )}
      <dl className="meta-list">
        <Meta label={t("Version")}>
          <span className="tabular">{row.version}</span>
        </Meta>
        {authors.length > 0 && (
          <Meta label={authors.length > 1 ? t("Authors") : t("Author")}>{authors.join(", ")}</Meta>
        )}
        <Meta label={t("Licence")}>
          {row.license_url ? (
            <ExternalLink href={row.license_url}>{row.license}</ExternalLink>
          ) : row.license ? (
            row.license
          ) : (
            <span className="hint">{t("None declared")}</span>
          )}{" "}
          <LicenseBadge row={row} />
        </Meta>
        <Meta label={t("Needs core")}>
          <code>{prettyReq(row.requires_core)}</code>{" "}
          <span className="hint">{t("you run {version}", { version: core })}</span>
        </Meta>
        <Meta label={t("Source")}>{sourceLabel(row.source)}</Meta>
        {row.homepage && (
          <Meta label={t("Homepage")}>
            <ExternalLink href={row.homepage}>
              {row.homepage.replace(/^https:\/\//, "")}
            </ExternalLink>
          </Meta>
        )}
        {row.repository && (
          <Meta label={t("Repository")}>
            <ExternalLink href={row.repository}>
              {row.repository.replace(/^https:\/\//, "")}
            </ExternalLink>
          </Meta>
        )}
        {row.documentation && (
          <Meta label={t("Documentation")}>
            <ExternalLink href={row.documentation}>
              {row.documentation.replace(/^https:\/\//, "")}
            </ExternalLink>
          </Meta>
        )}
        {row.support && (
          <Meta label={t("Support")}>
            <ExternalLink href={row.support}>{row.support.replace(/^https:\/\//, "")}</ExternalLink>
          </Meta>
        )}
        {row.categories.length > 0 && (
          <Meta label={t("Categories")}>
            <span className="chips">
              {row.categories.map((c) => (
                <Chip key={c}>{categoryLabel(c)}</Chip>
              ))}
            </span>
          </Meta>
        )}
        {row.keywords.length > 0 && (
          <Meta label={t("Keywords")}>
            <span className="chips">
              {row.keywords.map((k) => (
                <Chip key={k}>{k}</Chip>
              ))}
            </span>
          </Meta>
        )}
        {row.installed_at && (
          <Meta label={t("Installed")}>
            {installedAt(row)}
            {row.installed_by ? ` · ${row.installed_by}` : ""}
          </Meta>
        )}
        {row.update_available && (
          <Meta label={t("Update available")}>
            <span className="tabular">{row.update_available}</span>
          </Meta>
        )}
      </dl>
    </div>
  );
}

function Dependencies({
  row,
  core,
  title,
  nameOf,
  index,
  hrefFor,
}: {
  row: Row;
  core: string;
  title: string;
  nameOf: (id: string) => string;
  index: Map<string, Row>;
  hrefFor: (id: string) => string | null;
}) {
  const tree = buildTree(
    row.id,
    index.has(row.id) ? index : new Map(index).set(row.id, row),
    nameOf,
  );
  const coming = flattenTree(tree).filter(
    (n) => n.status === "will_install" || n.status === "will_upgrade",
  );
  const conflicts = row.dependencies.filter((d) =>
    ["conflict", "missing"].includes(dependencyStatus(d)),
  );
  return (
    <div className="stack gap-lg">
      <section className="stack" aria-labelledby="dep-ranges-h">
        <h3 id="dep-ranges-h" className="overline">
          {t("Versions it supports")}
        </h3>
        <div className="table-wrap">
          <table className="table dep-ranges">
            <caption className="sr-only">
              {t("The versions of the core and of other modules {name} supports", { name: title })}
            </caption>
            <thead>
              <tr>
                <th scope="col">{t("Needs")}</th>
                <th scope="col">{t("Supported versions")}</th>
                <th scope="col">{t("Here")}</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>{t("Core")}</td>
                <td>
                  <code>{prettyReq(row.requires_core)}</code>
                </td>
                <td>
                  <span className="tabular">{core}</span>{" "}
                  <Badge
                    tone={row.compatible ? "success" : "danger"}
                    dot={false}
                    className="mod-badge"
                  >
                    {row.compatible ? t("Supported") : t("Not supported")}
                  </Badge>
                </td>
              </tr>
              {row.dependencies.map((d) => (
                <tr key={d.id}>
                  <td>{nameOf(d.id) || d.name}</td>
                  <td>
                    <code>{prettyReq(d.requirement)}</code>
                  </td>
                  <td>
                    {d.installed_version ? (
                      <span className="tabular">{d.installed_version}</span>
                    ) : (
                      <span className="hint">{t("not installed")}</span>
                    )}{" "}
                    <Badge
                      tone={d.satisfied ? "success" : "danger"}
                      dot={false}
                      className="mod-badge"
                    >
                      {d.satisfied ? t("Supported") : t("Not supported")}
                    </Badge>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="hint">
          {t(
            "A module must say which versions of the core and of every module it needs it works with. An update that leaves a range is refused.",
          )}
        </p>
      </section>
      <section className="stack" aria-labelledby="dep-core-h">
        <h3 id="dep-core-h" className="overline">
          {t("Core")}
        </h3>
        <CoreRequirement
          name={title}
          requirement={row.requires_core}
          core={core}
          compatible={row.compatible}
        />
      </section>
      <section className="stack" aria-labelledby="dep-needs-h">
        <h3 id="dep-needs-h" className="overline">
          {t("Needs")}
        </h3>
        <DependencyTree
          nodes={tree}
          label={t("Dependencies of {name}", { name: title })}
          hrefFor={hrefFor}
        />
        {coming.length > 0 && (
          <Notice tone="info" title={t("Installing it also installs")}>
            {coming.map((n) => nameOf(n.id)).join(", ")}
          </Notice>
        )}
        {conflicts.length > 0 && (
          <Notice tone="danger" title={t("Something it needs is not available")}>
            {conflicts.map((d) => `${nameOf(d.id)} ${prettyReq(d.requirement)}`).join(", ")}
          </Notice>
        )}
      </section>
      <section className="stack" aria-labelledby="dep-by-h">
        <h3 id="dep-by-h" className="overline">
          {t("Required by")}
        </h3>
        {row.dependents.length === 0 ? (
          <p className="hint">{t("No installed module needs this one.")}</p>
        ) : (
          <ul className="plain dep-by">
            {row.dependents.map((id) => {
              const need = index.get(id)?.dependencies.find((d) => d.id === row.id);
              const href = hrefFor(id);
              return (
                <li key={id}>
                  {href ? <a href={href}>{nameOf(id)}</a> : <strong>{nameOf(id)}</strong>}
                  {need && (
                    <>
                      {" "}
                      <span className="hint">
                        {t("needs")} <code>{prettyReq(need.requirement)}</code>
                      </span>
                    </>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}

function ContentsTab({ row, kind }: { row: Row; kind: "installed" | "market" }) {
  const c: Contents | undefined = row.contents;
  if (!c)
    return (
      <p className="hint">
        {kind === "market"
          ? t("What a module contains is shown once it is installed.")
          : t("This module declares no contents.")}
      </p>
    );
  return (
    <div className="stack gap-lg">
      {c.personal_data_fields > 0 && (
        <Notice tone="warning" title={t("Holds personal data")}>
          {t(
            "{count, plural, one {# field is} other {# fields are}} marked as personal data. Masking applies to agents, but you decide who may read them.",
            { count: c.personal_data_fields },
          )}
        </Notice>
      )}
      <section className="stack" aria-labelledby="ct-models-h">
        <h3 id="ct-models-h" className="overline">
          {t("Records")} <span className="tab-count">{c.models.length}</span>
        </h3>
        <ul className="plain ct-models">
          {c.models.map((m) => (
            <li key={m.name}>
              <strong>{m.name}</strong>
              <span className="hint">
                {" "}
                · {t("{count, plural, one {# field} other {# fields}}", { count: m.fields })}
                {m.personal_fields > 0 &&
                  ` · ${t("{count} with personal data", { count: m.personal_fields })}`}
              </span>
              <p className="hint">{m.description}</p>
              {m.actions.length > 0 && (
                <span className="chips">
                  {m.actions.map((a) => (
                    <Chip key={a.name} title={a.description}>
                      {a.name}
                      {a.approval ? ` · ${t("approval")}` : ""}
                    </Chip>
                  ))}
                </span>
              )}
            </li>
          ))}
        </ul>
      </section>
      {c.approval_steps.length > 0 && (
        <section className="stack" aria-labelledby="ct-appr-h">
          <h3 id="ct-appr-h" className="overline">
            {t("Steps that need a human's approval")}
          </h3>
          <ul className="plain">
            {c.approval_steps.map((a) => (
              <li key={`${a.model}.${a.action}`}>
                <code>
                  {a.model}.{a.action}
                </code>{" "}
                <span className="hint">{t("group {group}", { group: a.group })}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
      <section className="stack" aria-labelledby="ct-groups-h">
        <h3 id="ct-groups-h" className="overline">
          {t("Permission groups")}
        </h3>
        <span className="chips">
          {c.groups.map((g) => (
            <Chip key={g}>
              <code>{g}</code>
            </Chip>
          ))}
        </span>
      </section>
      <section className="stack" aria-labelledby="ct-classes-h">
        <h3 id="ct-classes-h" className="overline">
          {t("Data classes")}
        </h3>
        <span className="chips">
          {Object.entries(c.data_classes).map(([k, n]) => (
            <Badge
              key={k}
              tone={k === "none" ? "neutral" : "info"}
              dot={false}
              className="mod-badge"
            >
              {k} · {n}
            </Badge>
          ))}
        </span>
      </section>
    </div>
  );
}

function Versions({
  row,
  kind,
  actions,
}: {
  row: Row;
  kind: "installed" | "market";
  actions: CardActions;
}) {
  const installed = kind === "installed" ? row.version : (row.installed_version ?? null);
  const available = kind === "installed" ? row.update_available : row.version;
  const blocked = kind === "installed" && row.update_available && row.update_compatible === false;
  return (
    <div className="stack gap-lg">
      {kind === "installed" && row.update_available && (
        <section className="stack" aria-labelledby="ver-upd-h">
          <h3 id="ver-upd-h" className="overline">
            {t("Update to {version}", { version: row.update_available })}
          </h3>
          {blocked ? (
            <Notice tone="warning" title={t("It cannot be installed here yet")}>
              {(row.update_blockers ?? []).map((b) => b.message).join(" ")}
            </Notice>
          ) : (
            <Notice tone="success" title={t("It fits this instance")}>
              {t("Every module that depends on this one still supports the new version.")}
            </Notice>
          )}
          <Notes text={row.update_changelog ?? ""} label={t("What is new")} />
          <ChangeList changes={row.update_changes ?? []} migrations={row.update_migrations ?? []} />
        </section>
      )}
      {kind === "installed" && row.rollback_version && (
        <section className="stack" aria-labelledby="ver-rb-h">
          <h3 id="ver-rb-h" className="overline">
            {t("Before the last update")}
          </h3>
          <p>
            {t("Version {version} is kept.", { version: row.rollback_version })}{" "}
            {row.upgraded_at ? (
              <span className="hint">
                {fmt.dateOf(new Date(row.upgraded_at * 1000), {
                  dateStyle: "medium",
                  timeStyle: "short",
                })}
              </span>
            ) : null}
          </p>
          {row.rollback_available ? (
            <div>
              <Button icon="undo" onClick={() => actions.onRollback(row)}>
                {t("Roll back to {version}", { version: row.rollback_version })}
              </Button>
            </div>
          ) : (
            <Notice tone="info" title={t("It cannot be rolled back from here")}>
              {row.rollback_blocker ?? ""}
            </Notice>
          )}
        </section>
      )}
      {kind === "installed" && row.changelog && (
        <Notes
          text={row.changelog}
          label={t("Notes of the installed version ({version})", { version: row.version })}
        />
      )}
      <div className="table-wrap">
        <table className="table">
          <caption className="sr-only">{t("Versions of this module")}</caption>
          <thead>
            <tr>
              <th scope="col">{t("Version")}</th>
              <th scope="col">{t("State")}</th>
            </tr>
          </thead>
          <tbody>
            {available && available !== installed && (
              <tr>
                <td className="tabular">{available}</td>
                <td>
                  <Badge tone="info" dot={false} className="mod-badge">
                    {t("In the marketplace")}
                  </Badge>
                </td>
              </tr>
            )}
            {installed && (
              <tr>
                <td className="tabular">{installed}</td>
                <td>
                  <Badge tone="success" dot={false} className="mod-badge">
                    {t("Installed")}
                  </Badge>
                  {row.installed_at ? <span className="hint"> · {installedAt(row)}</span> : null}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <p className="hint">
        {t(
          "An update is checked against every module that depends on this one before it runs, and it runs as one operation: it either completes or changes nothing. Rolling back is possible while the update has not deleted data.",
        )}
      </p>
    </div>
  );
}

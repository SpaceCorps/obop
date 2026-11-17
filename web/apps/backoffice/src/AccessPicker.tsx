import { useEffect, useId, useMemo, useState, type SyntheticEvent } from "react";
import { Checkbox, Input, Segmented, Switch, t } from "@obop/ui";
import { call } from "./api.ts";
import { AppIcon, type App } from "./Chrome.tsx";
import { account } from "./SecurityAccount.tsx";
import { pluralTitle } from "./nav.ts";
import {
  ADMIN,
  buildAccess,
  levelOf,
  prefixOf,
  withGroup,
  withLevel,
  type Access,
  type Ladder,
  type Level,
  type ModuleLike,
} from "./access.ts";
import { groupWord, levelWord, otherLabel } from "./accessText.ts";

/* Access by app (D69 amendment, PROPOSED): the picker that New person and "Access of NAME" share.
   One row per app with the choices No access, the base rung and the manage rung, built by
   access.ts from `module.list`. The agent-token dialog keeps the raw GroupPicker of People.tsx:
   a token scope is an exact set of groups. */

const TECHNICAL = "people-technical-names";
/** More apps than this and a filter is offered. */
const FILTER_ABOVE = 8;

/** What the picker needs from the server, loaded once: the module list and the known groups. */
export type AccessData = {
  rows: ModuleLike[];
  known: string[];
  /** A list could not be loaded: an empty list then means "unknown", not "none". */
  failed?: boolean;
};

export function useAccessData(token: string): AccessData {
  const [data, setData] = useState<AccessData>({ rows: [], known: [] });
  useEffect(() => {
    let live = true;
    void Promise.all([
      call(token, "module.list"),
      account<{ groups: string[] }>(token, "GET", "/groups"),
    ]).then(([modules, groups]) => {
      if (!live) return;
      setData({
        rows:
          modules.kind === "ok" && Array.isArray(modules.value)
            ? (modules.value as ModuleLike[])
            : [],
        known: groups.ok ? groups.value.groups : [],
        failed: modules.kind !== "ok" || !Array.isArray(modules.value) || !groups.ok,
      });
    });
    return () => {
      live = false;
    };
  }, [token]);
  return data;
}

/** The access of the launcher apps, for the table and the picker. */
export function useAccess(data: AccessData, apps: App[], held: string[] = []): Access {
  return useMemo(
    () =>
      buildAccess(
        data.rows,
        apps.map((a) => ({ id: a.id, name: a.name })),
        data.known,
        held,
      ),
    // `held` is a fresh array at every render; its content is what counts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data, apps, held.join("\n")],
  );
}

function readTechnical(): boolean {
  try {
    return localStorage.getItem(TECHNICAL) === "1";
  } catch {
    return false;
  }
}

/** "Invoices, Journals": the models that use a group, in the words of the screens that show them. */
function modelsOf(ladder: Ladder, group: string, apps: App[]): string {
  const names = new Set<string>();
  for (const [module, model] of ladder.uses[group] ?? []) {
    const meta = apps.find((a) => a.id === module)?.module.models.find((m) => m.name === model);
    names.add(meta ? pluralTitle(meta, module) : model.replace(/_/g, " "));
  }
  return [...names].slice(0, 6).join(", ");
}

/**
 * The open state of a fold ("More access", "Other access"). It starts open when the person holds
 * something in it and stays as the hand left it. `held` is derived from the lists, which arrive
 * after the first render, so the state keeps only what the hand did: null until it does something.
 * The `toggle` event also fires when React itself sets `open`; the handler writes the state only
 * when the element differs from what is rendered, so that echo changes nothing (no loop).
 */
function useFold(held: boolean) {
  const [toggled, setToggled] = useState<boolean | null>(null);
  const open = toggled ?? held;
  return {
    open,
    /** Called when a box inside is changed: the section stays open under the hand. */
    pin: () => setToggled(true),
    onToggle: (e: SyntheticEvent<HTMLDetailsElement>) => {
      const now = e.currentTarget.open;
      if (now !== open) setToggled(now);
    },
  };
}

function Technical({ show, group }: { show: boolean; group: string }) {
  return show ? (
    <>
      {" · "}
      <code>{group}</code>
    </>
  ) : null;
}

function AppRow({
  ladder,
  value,
  onChange,
  apps,
  technical,
  disabled,
}: {
  ladder: Ladder;
  value: string[];
  onChange: (next: string[]) => void;
  apps: App[];
  technical: boolean;
  /** An administrator needs no app rows: they stay as stored and cannot be changed meanwhile. */
  disabled: boolean;
}) {
  const level = levelOf(value, ladder);
  const app = apps.find((a) => a.id === ladder.id);
  const held = value.filter((g) => ladder.groups.includes(g));
  const fold = useFold(ladder.extras.some((g) => value.includes(g)));
  const options: { value: Level; label: string }[] = [
    { value: "none", label: t("No access") },
    { value: "user", label: levelWord(ladder, "user") },
    ...(ladder.manage ? [{ value: "manager" as Level, label: levelWord(ladder, "manager") }] : []),
  ];
  return (
    <fieldset className={disabled ? "access-app access-dim" : "access-app"} disabled={disabled}>
      <legend className="access-app-name">
        {app && <AppIcon app={app} size={28} />}
        <span>{ladder.name}</span>
      </legend>
      {ladder.manage === null ? (
        <Switch
          label={
            <>
              {t("Can use {app}", { app: ladder.name })}
              <Technical show={technical} group={ladder.base} />
            </>
          }
          checked={level === "user"}
          onChange={(e) => onChange(withLevel(value, ladder, e.target.checked ? "user" : "none"))}
        />
      ) : (
        <Segmented
          label={t("Access to {app}", { app: ladder.name })}
          options={options}
          value={level}
          onChange={(next) => onChange(withLevel(value, ladder, next))}
        />
      )}
      {technical && ladder.manage !== null && (
        <p className="hint access-tech">
          <code>{ladder.base}</code> · <code>{ladder.manage}</code>
        </p>
      )}
      {level === "custom" && (
        <p className="hint access-custom" role="note">
          {t("Custom: {groups} only. Choose a level above to fix it.", { groups: held.join(", ") })}
        </p>
      )}
      {ladder.extras.length > 0 && level !== "none" && (
        <details className="access-more" open={fold.open} onToggle={fold.onToggle}>
          <summary>{t("More access in {app}", { app: ladder.name })}</summary>
          <div className="stack">
            {ladder.extras.map((g) => {
              const models = modelsOf(ladder, g, apps);
              return (
                <Checkbox
                  key={g}
                  label={
                    <>
                      {prefixOf(g) === ladder.id ? groupWord(g) : otherLabel(g)}
                      <Technical show={technical} group={g} />
                    </>
                  }
                  hint={
                    models
                      ? t("Gives access to: {models}", { models })
                      : t("Needed for some actions and fields")
                  }
                  checked={value.includes(g)}
                  onChange={(e) => {
                    fold.pin();
                    onChange(withGroup(value, g, e.target.checked));
                  }}
                />
              );
            })}
          </div>
        </details>
      )}
    </fieldset>
  );
}

/**
 * The access of one person: the administrator switch, one row per app, the groups no app owns and
 * the technical names on demand. `value` is the whole group list the server will store, and every
 * choice returns it whole: a group the picker does not show is carried over unchanged.
 */
export function AccessPicker({
  access,
  value,
  onChange,
  apps = [],
  self = false,
  loadFailed = false,
}: {
  access: Access;
  value: string[];
  onChange: (next: string[]) => void;
  apps?: App[];
  /** Your own row: the administrator switch cannot be turned off here. */
  self?: boolean;
  /** The module or group list could not be loaded: say so instead of "no apps yet". */
  loadFailed?: boolean;
}) {
  const id = useId();
  const [technical, setTechnical] = useState(readTechnical);
  const [filter, setFilter] = useState("");
  const admin = value.includes(ADMIN);
  const locked = self && admin;
  const filtering = access.ladders.length > FILTER_ABOVE;
  const wanted = filtering ? filter.trim().toLowerCase() : "";
  const shown = access.ladders.filter((l) => l.name.toLowerCase().includes(wanted));
  const otherFold = useFold(access.other.some((g) => value.includes(g)));
  return (
    <div className="access-picker">
      <div className="access-admin">
        <Switch
          label={t("Administrator: can do everything")}
          aria-describedby={`${id}-admin`}
          checked={admin}
          disabled={locked}
          onChange={(e) => onChange(withGroup(value, ADMIN, e.target.checked))}
        />
        <p id={`${id}-admin`} className="hint">
          {locked
            ? t("You cannot take administrator access from yourself; ask another administrator.")
            : t("Can use every app, change settings and give access to other people.")}
        </p>
      </div>
      {filtering && (
        <Input
          label={t("Filter apps")}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          autoComplete="off"
        />
      )}
      {/* A disabled fieldset dims and locks its rows; the choices stay as stored, so turning the
          switch off brings them back. */}
      <div className="access-apps">
        {shown.map((l) => (
          <AppRow
            key={l.id}
            ladder={l}
            value={value}
            onChange={onChange}
            apps={apps}
            technical={technical}
            disabled={admin}
          />
        ))}
        {loadFailed && (
          <p className="hint" role="note">
            {t("Could not load the apps.")}
          </p>
        )}
        {!loadFailed && shown.length === 0 && (
          <p className="hint">
            {filtering && wanted ? t("No app matches.") : t("No apps to give access to yet.")}
          </p>
        )}
      </div>
      {access.other.length > 0 && (
        <details className="access-other" open={otherFold.open} onToggle={otherFold.onToggle}>
          <summary>{t("Other access ({count})", { count: access.other.length })}</summary>
          <div className="stack">
            {access.other.map((g) => (
              <Checkbox
                key={g}
                label={
                  <>
                    {access.stale.has(g)
                      ? t("No longer installed: {group}", { group: g })
                      : otherLabel(g)}
                    <Technical show={technical && !access.stale.has(g)} group={g} />
                  </>
                }
                checked={value.includes(g)}
                onChange={(e) => {
                  otherFold.pin();
                  onChange(withGroup(value, g, e.target.checked));
                }}
              />
            ))}
          </div>
        </details>
      )}
      <Switch
        label={t("Show technical names")}
        checked={technical}
        onChange={(e) => {
          setTechnical(e.target.checked);
          try {
            localStorage.setItem(TECHNICAL, e.target.checked ? "1" : "0");
          } catch {
            // A browser that forbids storage keeps the choice for this dialog only.
          }
        }}
      />
    </div>
  );
}

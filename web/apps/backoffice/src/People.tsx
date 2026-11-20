import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  Dialog,
  Input,
  Menu,
  Notice,
  Table,
  t,
  twoFactorMessage,
  tx,
  useWorded,
} from "@obop/ui";
import { AccessPicker, useAccess, useAccessData, type AccessData } from "./AccessPicker.tsx";
import { levelWord } from "./accessText.ts";
import { summarise, type Access } from "./access.ts";
import { AppIcon, type App } from "./Chrome.tsx";
import { MIN_PASSWORD, account, reason, type Failure, type Notify } from "./SecurityAccount.tsx";
import { MicrosoftDialog } from "./PeopleSso.tsx";
import { Refusal } from "./Refusal.tsx";
import { readSso, type SsoState } from "./SsoAdmin.tsx";

/* D69: the people of the instance, for administrators: create a person, choose what they may use
   (access by app, AccessPicker.tsx), disable, enable, reset a password or a second factor, and
   offboard (disable and end every session, agent token and connected app for good). Everything
   goes through /api/auth/users. */

const ADMIN = "platform.admin";

type Person = {
  name: string;
  groups: string[];
  admin: boolean;
  disabled: boolean;
  offboarded: boolean;
  mfa_enabled: boolean;
  must_change_password: boolean;
  sessions: number;
  /** Signs in with Microsoft only (D91). */
  sso_only?: boolean;
  /** The object id at Microsoft the account is bound to; absent when it is not bound. */
  sso_oid?: string | null;
};

function sortGroups(all: string[]): string[] {
  return [...new Set(all)].sort((a, b) =>
    a === ADMIN ? -1 : b === ADMIN ? 1 : a.localeCompare(b, undefined, { numeric: true }),
  );
}

/** A checkbox list of permission groups with a filter. */
export function GroupPicker({
  groups,
  value,
  onChange,
  filterAbove,
}: {
  groups: string[];
  value: string[];
  onChange: (next: string[]) => void;
  /** Show the filter only when there are more groups than this. Not given: always. */
  filterAbove?: number;
}) {
  const [filter, setFilter] = useState("");
  const all = sortGroups([...groups, ...value]);
  const filtering = filterAbove === undefined || all.length > filterAbove;
  const wanted = filtering ? filter.trim().toLowerCase() : "";
  const shown = all.filter((g) => g.toLowerCase().includes(wanted));
  return (
    <fieldset className="group-picker">
      <legend>{t("Permission groups")}</legend>
      {filtering && (
        <Input
          label={t("Filter groups")}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          autoComplete="off"
        />
      )}
      <div className="group-list" role="group" aria-label={t("Permission groups")}>
        {shown.map((g) => (
          <Checkbox
            key={g}
            label={g === ADMIN ? t("{group} (administrator: every permission)", { group: g }) : g}
            checked={value.includes(g)}
            onChange={(e) =>
              onChange(e.target.checked ? [...value, g] : value.filter((x) => x !== g))
            }
          />
        ))}
        {shown.length === 0 && <p className="hint">{t("No group matches.")}</p>}
      </div>
    </fieldset>
  );
}

function NewPersonDialog({
  token,
  data,
  apps,
  onClose,
  onDone,
  failed,
}: {
  token: string;
  data: AccessData;
  apps: App[];
  onClose: () => void;
  onDone: () => void;
  failed: (r: Failure) => string;
}) {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const access = useAccess(data, apps, picked);
  const [busy, setBusy] = useState(false);
  // Kept as how to word it, so a switch of the language words it again (D9).
  const [error, setError] = useWorded();
  const [temp, setTemp] = useState<string | null>(null);
  const tooShort = password !== "" && [...password].length < MIN_PASSWORD;

  async function submit() {
    setBusy(true);
    setError(null);
    const r = await account<{ user: string; temporary_password?: string }>(
      token,
      "POST",
      "/users",
      { username: name.trim(), groups: picked, ...(password ? { password } : {}) },
    );
    setBusy(false);
    if (!r.ok) return setError(() => failed(r));
    onDone();
    if (r.value.temporary_password) setTemp(r.value.temporary_password);
    else onClose();
  }

  if (temp)
    return (
      <Dialog title={t("Temporary password")} size="sm" onClose={onClose}>
        <div className="stack">
          <Notice tone="warning" title={t("Shown once")}>
            {t(
              "Give {user} this password in person. They choose their own at the first sign-in. It cannot be shown again.",
              { user: name.trim().toLowerCase() },
            )}
          </Notice>
          <code className="temp-password" data-testid="temp-password">
            {temp}
          </code>
          <div className="actionrow">
            <Button
              onClick={() => void navigator.clipboard?.writeText(temp).catch(() => undefined)}
            >
              {t("Copy")}
            </Button>
            <Button variant="primary" onClick={onClose}>
              {t("Done")}
            </Button>
          </div>
        </div>
      </Dialog>
    );

  return (
    <Dialog
      title={t("New person")}
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!name.trim() || tooShort}
            onClick={() => void submit()}
          >
            {t("Create person")}
          </Button>
        </>
      }
    >
      <form
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim() && !tooShort) void submit();
        }}
      >
        <Input
          label={t("Username")}
          hint={t("Letters, digits and . _ - @")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoComplete="off"
          required
        />
        <Input
          label={t("Password (optional)")}
          hint={t(
            "At least {n} characters. Leave it empty to get a temporary password, shown once. Either way the person must choose their own at the first sign-in.",
            { n: MIN_PASSWORD },
          )}
          type="password"
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <AccessPicker
          access={access}
          value={picked}
          onChange={setPicked}
          apps={apps}
          loadFailed={data.failed}
        />
        {error && <Refusal message={error} />}
      </form>
    </Dialog>
  );
}

function AccessDialog({
  token,
  person,
  data,
  apps,
  self,
  onClose,
  onDone,
  failed,
}: {
  token: string;
  person: Person;
  data: AccessData;
  apps: App[];
  /** This is your own row. */
  self: boolean;
  onClose: () => void;
  onDone: () => void;
  failed: (r: Failure) => string;
}) {
  const [picked, setPicked] = useState<string[]>(person.groups);
  const access = useAccess(data, apps, person.groups);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useWorded();
  const same = useMemo(
    () => picked.length === person.groups.length && picked.every((g) => person.groups.includes(g)),
    [picked, person.groups],
  );
  async function save() {
    setBusy(true);
    setError(null);
    const r = await account(token, "POST", `/users/${encodeURIComponent(person.name)}/groups`, {
      groups: picked,
    });
    setBusy(false);
    if (r.ok) {
      onDone();
      onClose();
    } else setError(() => failed(r));
  }
  return (
    <Dialog
      title={t("Access of {user}", { user: person.name })}
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button variant="primary" loading={busy} disabled={same} onClick={() => void save()}>
            {t("Save access")}
          </Button>
        </>
      }
    >
      <div className="stack">
        <p className="hint">
          {t(
            "Takes effect on their next request, in every session they have open. Recorded in the security log with the groups before and after.",
          )}
        </p>
        <AccessPicker
          access={access}
          value={picked}
          onChange={setPicked}
          apps={apps}
          self={self}
          loadFailed={data.failed}
        />
        {error && <Refusal message={error} />}
      </div>
    </Dialog>
  );
}

/** What the Access cell of a row says: chips, at most three apps, then "+N more". */
function AccessCell({
  person,
  access,
  apps,
  onOpen,
}: {
  person: Person;
  access: Access;
  apps: App[];
  onOpen: () => void;
}) {
  const s = summarise(person.groups, access);
  if (s.admin) return <span className="chip accent">{t("Everything")}</span>;
  const shown = s.apps.slice(0, 3);
  const hidden = s.apps.length - shown.length + (s.other.length > 0 && s.apps.length > 0 ? 1 : 0);
  if (s.apps.length === 0 && s.other.length === 0)
    return <Badge tone="warning">{t("No apps yet")}</Badge>;
  return (
    <div className="access-chips">
      {shown.map(({ ladder, level }) => {
        const app = apps.find((a) => a.id === ladder.id);
        return (
          <span key={ladder.id} className="chip">
            {app && <AppIcon app={app} size={16} />}
            {t("{app} · {level}", { app: ladder.name, level: levelWord(ladder, level) })}
          </span>
        );
      })}
      {s.apps.length === 0 && <span className="chip">{t("Other access")}</span>}
      {hidden > 0 && (
        <button
          type="button"
          className="chip access-more-chip"
          aria-label={`${t("+{count} more", { count: hidden })}, ${t("Access of {user}", { user: person.name })}`}
          onClick={onOpen}
        >
          {t("+{count} more", { count: hidden })}
        </button>
      )}
    </div>
  );
}

/** Administrators: the people of the instance. */
export function PeopleSection({
  token,
  notify,
  refreshKey = 0,
  me,
  onSignedOut,
  apps = [],
  onTwoFactorReset,
}: {
  token: string;
  notify: Notify;
  /** The launcher apps, in order: the names, icons and order of the access rows. */
  apps?: App[];
  /** A person's two-factor was reset: the security log has a new line. */
  onTwoFactorReset?: () => void;
  /** Who is signed in: a change of their own Microsoft sign-in says so. */
  me?: string;
  /** Changes when Microsoft sign-in was set up, changed or removed: the column follows. */
  refreshKey?: number;
  /** The server no longer knows this session (401), or an own change ended it: go to sign-in. */
  onSignedOut?: () => void;
}) {
  const id = useId();
  const [sso, setSso] = useState<SsoState>({ kind: "loading" });
  const [microsoft, setMicrosoft] = useState<string | null>(null);
  const [people, setPeople] = useState<Person[] | null>(null);
  const [resetting, setResetting] = useState<string | null>(null);
  const [temp, setTemp] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Person | null>(null);
  const [offboarding, setOffboarding] = useState<Person | null>(null);
  const [mfaReset, setMfaReset] = useState<Person | null>(null);
  const [mfaError, setMfaError] = useWorded();
  const data = useAccessData(token);
  const access = useAccess(data, apps);
  const [note, setNote] = useState("");
  const [offError, setOffError] = useState<string | null>(null);
  // Read at the moment of use: the parent may pass a new function at every render.
  const signedOut = useRef(onSignedOut);
  signedOut.current = onSignedOut;
  const load = useCallback(async () => {
    const r = await account<{ users: Person[] }>(token, "GET", "/users");
    // A 401 is a signed-out session, not an empty list: never draw it as a table with no one in it.
    if (!r.ok && r.status === 401 && signedOut.current) return signedOut.current();
    setPeople(r.ok ? r.value.users : []);
  }, [token]);
  useEffect(() => void load(), [load]);
  useEffect(() => {
    void readSso(token).then(setSso);
  }, [token, refreshKey]);
  const failed = (r: Failure) =>
    reason(r) || (r.status === 400 ? r.message : "") || t("Could not save");
  async function toggle(p: Person) {
    setBusy(true);
    const r = await account(
      token,
      "POST",
      `/users/${encodeURIComponent(p.name)}/${p.disabled ? "enable" : "disable"}`,
      {},
    );
    setBusy(false);
    if (r.ok) {
      notify(
        p.disabled
          ? t("{user} can sign in again.", { user: p.name })
          : t("{user} is disabled and signed out.", { user: p.name }),
      );
      void load();
    } else notify(failed(r), "error");
  }
  async function offboard(p: Person) {
    setBusy(true);
    setOffError(null);
    const r = await account<{
      revoked_sessions: number;
      revoked_agent_tokens: number;
      revoked_apps: number;
    }>(token, "POST", `/users/${encodeURIComponent(p.name)}/offboard`, { note });
    setBusy(false);
    if (r.ok) {
      notify(
        t(
          "{user} is offboarded: {sessions} session(s), {tokens} agent token(s) and {apps} connected app(s) ended for good.",
          {
            user: p.name,
            sessions: r.value.revoked_sessions,
            tokens: r.value.revoked_agent_tokens,
            apps: r.value.revoked_apps,
          },
        ),
      );
      setOffboarding(null);
      setNote("");
      void load();
    } else setOffError(failed(r));
  }
  async function resetTwoFactor(p: Person) {
    setBusy(true);
    setMfaError(null);
    const r = await account(token, "POST", "/mfa/reset", { user: p.name });
    setBusy(false);
    if (r.ok) {
      notify(t("Two-factor reset for {user}", { user: p.name }));
      setMfaReset(null);
      void load();
      onTwoFactorReset?.();
    } else
      setMfaError(() =>
        r.status === 400
          ? t("There is no such person, or they have no two-factor to reset.")
          : twoFactorMessage({ status: r.status, code: r.code }),
      );
  }
  const mine = (me ?? "").toLowerCase();
  // An instance that does not use Microsoft sees no change: the column and the button need a
  // provider or at least one binding.
  const microsoftShown =
    sso.kind === "set" || (people ?? []).some((p) => p.sso_oid || p.sso_only === true);
  const shownPerson = people?.find((p) => p.name === microsoft) ?? null;
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <div className="people-head">
        <h3 id={`${id}-h`} className="overline">
          {t("People")}
        </h3>
        <Button size="sm" onClick={() => setCreating(true)}>
          {t("New person")}
        </Button>
      </div>
      {people !== null && (
        <Table caption={t("People")} dense stack className="people-table">
          <thead>
            <tr>
              <th scope="col">{t("Person")}</th>
              <th scope="col">{tx("access", "Access")}</th>
              <th scope="col">{t("Two-factor")}</th>
              <th scope="col">{t("Sessions")}</th>
              <th scope="col">{t("Status")}</th>
              {microsoftShown && <th scope="col">{t("Microsoft")}</th>}
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {people.map((p) => (
              <tr key={p.name} data-person={p.name}>
                <td>{p.name}</td>
                <td
                  className="access-cell"
                  data-label={tx("access", "Access")}
                  title={p.groups.join(", ")}
                >
                  <AccessCell person={p} access={access} apps={apps} onOpen={() => setEditing(p)} />
                </td>
                <td data-label={t("Two-factor")}>{p.mfa_enabled ? t("On") : t("Off")}</td>
                <td data-label={t("Sessions")}>{p.sessions}</td>
                {/* No label on a phone: the badge sits beside the name and says what it is. */}
                <td>
                  {p.offboarded ? (
                    <Badge tone="danger">{t("Offboarded")}</Badge>
                  ) : p.disabled ? (
                    <Badge tone="danger">{t("Disabled")}</Badge>
                  ) : p.must_change_password ? (
                    <Badge tone="warning">{t("Must choose a password")}</Badge>
                  ) : (
                    <Badge tone="success">{t("Active")}</Badge>
                  )}
                </td>
                {microsoftShown && (
                  <td data-label={t("Microsoft")}>
                    {/* The cell is the control: its words are the state, its name has the person. */}
                    <Button
                      size="sm"
                      variant="ghost"
                      className="sso-cell"
                      aria-label={
                        p.sso_only
                          ? t("Microsoft sign-in for {user}: Microsoft only", { user: p.name })
                          : p.sso_oid
                            ? t("Microsoft sign-in for {user}: bound", { user: p.name })
                            : t("Microsoft sign-in for {user}: not bound", { user: p.name })
                      }
                      onClick={() => setMicrosoft(p.name)}
                    >
                      {p.sso_only ? t("Microsoft only") : p.sso_oid ? t("Bound") : t("Not bound")}
                    </Button>
                  </td>
                )}
                <td className="cell-actions">
                  <div className="actionrow">
                    <Button
                      size="sm"
                      aria-label={t("Access of {user}", { user: p.name })}
                      onClick={() => setEditing(p)}
                    >
                      {tx("access", "Access")}
                    </Button>
                    <Menu
                      ariaLabel={t("More actions for {user}", { user: p.name })}
                      align="end"
                      fixed
                      triggerClassName="btn small"
                      trigger={<>{tx("access", "More")}</>}
                      items={[
                        {
                          label: t("Reset password"),
                          onSelect: () => {
                            setResetting(p.name);
                            setTemp("");
                            setError(null);
                          },
                        },
                        ...(p.mfa_enabled && !p.offboarded && p.name.toLowerCase() !== mine
                          ? [
                              {
                                label: t("Reset two-factor"),
                                onSelect: () => {
                                  setMfaReset(p);
                                  setMfaError(null);
                                },
                              },
                            ]
                          : []),
                        {
                          label: p.disabled ? t("Enable") : t("Disable"),
                          disabled: busy,
                          onSelect: () => void toggle(p),
                        },
                        ...(p.offboarded
                          ? []
                          : [
                              "separator" as const,
                              {
                                label: t("Offboard"),
                                danger: true,
                                onSelect: () => {
                                  setOffboarding(p);
                                  setNote("");
                                  setOffError(null);
                                },
                              },
                            ]),
                      ]}
                    />
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
      {resetting && (
        // A new form for each person, so its field takes the focus and is scrolled to: on a phone
        // it renders far below the row that was tapped.
        <form
          key={resetting}
          className="stack"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            setError(null);
            const r = await account(
              token,
              "POST",
              `/users/${encodeURIComponent(resetting)}/reset-password`,
              {
                password: temp,
              },
            );
            setBusy(false);
            if (r.ok) {
              notify(
                t("Temporary password set for {user}. They choose a new one when they sign in.", {
                  user: resetting,
                }),
              );
              setResetting(null);
              setTemp("");
              void load();
            } else setError(failed(r));
          }}
        >
          <Input
            label={t("Temporary password for {user}", { user: resetting })}
            hint={t(
              "At least {n} characters. Tell them in person. They are signed out everywhere and must choose a new password at the next sign-in. Recorded in the security log.",
              { n: MIN_PASSWORD },
            )}
            type="password"
            name="temporary_password"
            autoComplete="new-password"
            autoFocus
            value={temp}
            onChange={(e) => setTemp(e.target.value)}
            error={error ?? undefined}
            required
          />
          <div className="actionrow">
            <Button
              type="submit"
              variant="primary"
              loading={busy}
              disabled={[...temp].length < MIN_PASSWORD}
            >
              {t("Set temporary password")}
            </Button>
            <Button variant="ghost" onClick={() => setResetting(null)}>
              {t("Cancel")}
            </Button>
          </div>
        </form>
      )}
      {creating && (
        <NewPersonDialog
          token={token}
          data={data}
          apps={apps}
          failed={failed}
          onClose={() => setCreating(false)}
          onDone={() => {
            notify(t("Person created."));
            void load();
          }}
        />
      )}
      {editing && (
        <AccessDialog
          key={editing.name}
          token={token}
          person={editing}
          data={data}
          apps={apps}
          self={editing.name.toLowerCase() === mine}
          failed={failed}
          onClose={() => setEditing(null)}
          onDone={() => {
            notify(t("Access of {user} saved.", { user: editing.name }));
            void load();
          }}
        />
      )}
      {shownPerson && (
        <MicrosoftDialog
          token={token}
          person={shownPerson}
          sso={sso}
          me={me}
          notify={notify}
          onClose={() => setMicrosoft(null)}
          onDone={() => void load()}
          onSignedOut={onSignedOut}
        />
      )}
      {mfaReset && (
        <ConfirmDialog
          title={t("Reset two-factor for {user}?", { user: mfaReset.name })}
          confirmLabel={t("Reset two-factor")}
          busy={busy}
          error={mfaError}
          onConfirm={() => void resetTwoFactor(mfaReset)}
          onClose={() => setMfaReset(null)}
        >
          <p>
            {t(
              "{user} is signed out everywhere and loses their authenticator app, passkeys and recovery codes. At the next sign-in they set up two-factor again. Use this when they lost their phone and their recovery codes. Recorded in the security log.",
              { user: mfaReset.name },
            )}
          </p>
        </ConfirmDialog>
      )}
      {offboarding && (
        <ConfirmDialog
          title={t("Offboard {user}?", { user: offboarding.name })}
          confirmLabel={t("Offboard")}
          busy={busy}
          error={offError}
          onConfirm={() => void offboard(offboarding)}
          onClose={() => setOffboarding(null)}
        >
          <div className="stack">
            <p>
              {t(
                "This disables the account and ends every session, agent token and connected app it has, for good. Enabling the account later lets the person sign in again, but those credentials stay revoked. Recorded in the security log.",
              )}
            </p>
            <Input
              label={t("Note (optional)")}
              hint={t("A short reason for the security log. No passwords or secrets.")}
              value={note}
              maxLength={200}
              onChange={(e) => setNote(e.target.value)}
              autoComplete="off"
            />
          </div>
        </ConfirmDialog>
      )}
    </section>
  );
}

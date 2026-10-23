import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { Badge, Button, Checkbox, ConfirmDialog, Dialog, Input, Notice, Table, t } from "@obop/ui";
import { MIN_PASSWORD, account, reason, type Failure, type Notify } from "./SecurityAccount.tsx";

/* D69: the people of the instance, for administrators: create a person, choose their permission
   groups, disable, enable, reset a password, and offboard (disable and end every session, agent
   token and connected app for good). Everything goes through /api/auth/users. */

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
};

/** The groups an account can be given, sorted with the administrator group first. */
function useGroups(token: string, open: boolean) {
  const [groups, setGroups] = useState<string[]>([]);
  useEffect(() => {
    if (!open) return;
    void account<{ groups: string[] }>(token, "GET", "/groups").then((r) => {
      if (r.ok) setGroups(r.value.groups);
    });
  }, [token, open]);
  return groups;
}

function sortGroups(all: string[]): string[] {
  return [...new Set(all)].sort((a, b) =>
    a === ADMIN ? -1 : b === ADMIN ? 1 : a.localeCompare(b, undefined, { numeric: true }),
  );
}

/** A checkbox list of permission groups with a filter. */
function GroupPicker({
  groups,
  value,
  onChange,
}: {
  groups: string[];
  value: string[];
  onChange: (next: string[]) => void;
}) {
  const [filter, setFilter] = useState("");
  const shown = sortGroups([...groups, ...value]).filter((g) =>
    g.toLowerCase().includes(filter.trim().toLowerCase()),
  );
  return (
    <fieldset className="group-picker">
      <legend>{t("Permission groups")}</legend>
      <Input
        label={t("Filter groups")}
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        autoComplete="off"
      />
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
  onClose,
  onDone,
  failed,
}: {
  token: string;
  onClose: () => void;
  onDone: () => void;
  failed: (r: Failure) => string;
}) {
  const groups = useGroups(token, true);
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
    if (!r.ok) return setError(failed(r));
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
        <GroupPicker groups={groups} value={picked} onChange={setPicked} />
        {error && (
          <div className="confirm-error" role="alert">
            {error}
          </div>
        )}
      </form>
    </Dialog>
  );
}

function GroupsDialog({
  token,
  person,
  onClose,
  onDone,
  failed,
}: {
  token: string;
  person: Person;
  onClose: () => void;
  onDone: () => void;
  failed: (r: Failure) => string;
}) {
  const known = useGroups(token, true);
  const [picked, setPicked] = useState<string[]>(person.groups);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
    } else setError(failed(r));
  }
  return (
    <Dialog
      title={t("Groups of {user}", { user: person.name })}
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button variant="primary" loading={busy} disabled={same} onClick={() => void save()}>
            {t("Save groups")}
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
        <GroupPicker groups={known} value={picked} onChange={setPicked} />
        {error && (
          <div className="confirm-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** Administrators: the people of the instance. */
export function PeopleSection({ token, notify }: { token: string; notify: Notify }) {
  const id = useId();
  const [people, setPeople] = useState<Person[] | null>(null);
  const [resetting, setResetting] = useState<string | null>(null);
  const [temp, setTemp] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Person | null>(null);
  const [offboarding, setOffboarding] = useState<Person | null>(null);
  const [note, setNote] = useState("");
  const [offError, setOffError] = useState<string | null>(null);
  const load = useCallback(async () => {
    const r = await account<{ users: Person[] }>(token, "GET", "/users");
    setPeople(r.ok ? r.value.users : []);
  }, [token]);
  useEffect(() => void load(), [load]);
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
        <Table caption={t("People")} dense className="people-table">
          <thead>
            <tr>
              <th scope="col">{t("Person")}</th>
              <th scope="col">{t("Role")}</th>
              <th scope="col">{t("Groups")}</th>
              <th scope="col">{t("Two-factor")}</th>
              <th scope="col">{t("Sessions")}</th>
              <th scope="col">{t("Status")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {people.map((p) => (
              <tr key={p.name} data-person={p.name}>
                <td>{p.name}</td>
                <td>{p.admin ? t("Administrator") : t("Member")}</td>
                <td className="group-cell" title={p.groups.join(", ")}>
                  {p.groups.length === 0 ? (
                    <span className="hint">{t("None")}</span>
                  ) : (
                    p.groups.join(", ")
                  )}
                </td>
                <td>{p.mfa_enabled ? t("On") : t("Off")}</td>
                <td>{p.sessions}</td>
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
                <td>
                  <div className="actionrow">
                    <Button size="sm" variant="ghost" onClick={() => setEditing(p)}>
                      {t("Groups")}
                    </Button>
                    <Button size="sm" disabled={busy} onClick={() => void toggle(p)}>
                      {p.disabled ? t("Enable") : t("Disable")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        setResetting(resetting === p.name ? null : p.name);
                        setTemp("");
                        setError(null);
                      }}
                    >
                      {t("Reset password")}
                    </Button>
                    {!p.offboarded && (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="danger-text"
                        onClick={() => {
                          setOffboarding(p);
                          setNote("");
                          setOffError(null);
                        }}
                      >
                        {t("Offboard")}
                      </Button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
      {resetting && (
        <form
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
          failed={failed}
          onClose={() => setCreating(false)}
          onDone={() => {
            notify(t("Person created."));
            void load();
          }}
        />
      )}
      {editing && (
        <GroupsDialog
          token={token}
          person={editing}
          failed={failed}
          onClose={() => setEditing(null)}
          onDone={() => {
            notify(t("Groups of {user} saved.", { user: editing.name }));
            void load();
          }}
        />
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

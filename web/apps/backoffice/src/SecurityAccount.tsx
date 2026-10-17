import { useCallback, useEffect, useId, useState } from "react";
import { Badge, Button, Input, Notice, Table, createTwoFactorApi, fmt, t } from "@obop/ui";
import { Icon, Logo } from "./ui.tsx";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/* B7: the account side of Customize > Security: change your password, see and end your
   sessions, and (administrators) the people of the instance with disable, enable and a password
   reset. Everything goes through the account routes under /api/auth. */

type Failure = { status: number; code: string; message: string };
type Outcome<T> = { ok: true; value: T } | ({ ok: false } & Failure);

async function account<T>(
  token: string,
  method: "GET" | "POST",
  path: string,
  body?: object,
): Promise<Outcome<T>> {
  try {
    const res = await fetch(`/api/auth${path}`, {
      method,
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const json = (await res.json().catch(() => ({}))) as { code?: string; error?: string };
    if (res.ok) return { ok: true, value: json as T };
    return { ok: false, status: res.status, code: json.code ?? "", message: json.error ?? "" };
  } catch {
    return { ok: false, status: 0, code: "network", message: "" };
  }
}

/** A refusal in the person's language. */
function reason(f: Failure): string {
  if (f.code === "invalid_password") return t("The current password is not right.");
  if (f.code === "invalid_code") return t("That code is not valid.");
  if (f.status === 429) return t("Too many attempts. Wait a minute and try again.");
  if (f.status === 0) return t("The server could not be reached.");
  if (f.status === 404) return t("You are not allowed to do this.");
  if (f.status === 401) return t("You are signed out. Sign in again.");
  return "";
}

const MIN_PASSWORD = 12;

/** Change your own password. Your other sessions are signed out. */
export function PasswordSection({
  token,
  twoFactor,
  notify,
  onDone,
  title,
}: {
  token: string;
  /** Two-factor is on: a code is needed too. */
  twoFactor: boolean;
  notify: Notify;
  onDone?: () => void;
  title?: string;
}) {
  const id = useId();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mismatch = again !== "" && again !== next;
  const tooShort = next !== "" && [...next].length < MIN_PASSWORD;
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {title ?? t("Password")}
      </h3>
      <form
        className="stack"
        onSubmit={async (e) => {
          e.preventDefault();
          if (mismatch || tooShort) return;
          setBusy(true);
          setError(null);
          const factor = code.replace(/\s/g, "");
          const r = await account<{ changed: boolean }>(token, "POST", "/password", {
            current_password: current,
            new_password: next,
            ...(twoFactor
              ? factor.length > 8
                ? { recovery_code: factor }
                : { code: factor }
              : {}),
          });
          setBusy(false);
          if (r.ok) {
            setCurrent("");
            setNext("");
            setAgain("");
            setCode("");
            notify(t("Password changed. Your other sessions were signed out."));
            onDone?.();
          } else {
            setError(
              reason(r) ||
                t("Choose a different password of at least {n} characters.", { n: MIN_PASSWORD }),
            );
          }
        }}
      >
        <Input
          label={t("Current password")}
          type="password"
          name="current_password"
          autoComplete="current-password"
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
          required
        />
        <Input
          label={t("New password")}
          hint={t("At least {n} characters.", { n: MIN_PASSWORD })}
          type="password"
          name="new_password"
          autoComplete="new-password"
          value={next}
          onChange={(e) => setNext(e.target.value)}
          error={tooShort ? t("At least {n} characters.", { n: MIN_PASSWORD }) : undefined}
          required
        />
        <Input
          label={t("Repeat the new password")}
          type="password"
          name="repeat_password"
          autoComplete="new-password"
          value={again}
          onChange={(e) => setAgain(e.target.value)}
          error={mismatch ? t("The new passwords do not match.") : undefined}
          required
        />
        {twoFactor && (
          <Input
            label={t("Authenticator code")}
            hint={t("From your authenticator app, or one of your recovery codes.")}
            name="code"
            inputMode="numeric"
            autoComplete="one-time-code"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            required
          />
        )}
        {error && (
          <p className="hint" role="alert">
            {error}
          </p>
        )}
        <div className="actionrow">
          <Button
            type="submit"
            variant="primary"
            loading={busy}
            disabled={!current || !next || !again || mismatch || tooShort}
          >
            {t("Change password")}
          </Button>
        </div>
      </form>
    </section>
  );
}

type SessionRow = {
  id: string;
  created_at: string | null;
  expires_at: string | null;
  current: boolean;
};

/** Where you are signed in, with a way out of all of it. */
export function SessionsSection({
  token,
  notify,
  onSignOut,
}: {
  token: string;
  notify: Notify;
  onSignOut: () => void;
}) {
  const id = useId();
  const [rows, setRows] = useState<SessionRow[] | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(async () => {
    const r = await account<{ sessions: SessionRow[] }>(token, "GET", "/sessions");
    setRows(r.ok ? r.value.sessions : []);
  }, [token]);
  useEffect(() => void load(), [load]);
  const others = (rows ?? []).filter((s) => !s.current).length;
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Your sessions")}
      </h3>
      {rows !== null && (
        <Table caption={t("Your sessions")} dense>
          <thead>
            <tr>
              <th scope="col">{t("Signed in")}</th>
              <th scope="col">{t("Ends")}</th>
              <th scope="col">{t("This device")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((s) => (
              <tr key={s.id}>
                <td>{s.created_at ? fmt.dateTime(s.created_at) : t("Earlier")}</td>
                <td>{s.expires_at ? fmt.dateTime(s.expires_at) : ""}</td>
                <td>{s.current ? <Badge tone="success">{t("This session")}</Badge> : null}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
      <p className="hint">
        {t(
          "Signing out everywhere ends every session of yours, including this one. Agent tokens you issued keep working until you revoke them.",
        )}
      </p>
      <div className="actionrow">
        <Button
          loading={busy}
          disabled={others === 0}
          onClick={async () => {
            setBusy(true);
            const r = await account<{ revoked: number }>(token, "POST", "/sessions/revoke-all", {
              keep_current: true,
            });
            setBusy(false);
            if (r.ok) {
              notify(
                t(
                  "{count, plural, one {# other session signed out.} other {# other sessions signed out.}}",
                  {
                    count: r.value.revoked,
                  },
                ),
              );
              void load();
            } else notify(reason(r) || t("Could not sign out."), "error");
          }}
        >
          {t("Sign out other sessions")}
        </Button>
        <Button
          variant="danger"
          loading={busy}
          onClick={async () => {
            setBusy(true);
            const r = await account<{ revoked: number }>(token, "POST", "/sessions/revoke-all", {});
            setBusy(false);
            if (r.ok) onSignOut();
            else notify(reason(r) || t("Could not sign out."), "error");
          }}
        >
          {t("Sign out everywhere")}
        </Button>
      </div>
    </section>
  );
}

type Person = {
  name: string;
  admin: boolean;
  disabled: boolean;
  mfa_enabled: boolean;
  must_change_password: boolean;
  sessions: number;
};

/** Administrators: the people of the instance. */
export function PeopleSection({ token, notify }: { token: string; notify: Notify }) {
  const id = useId();
  const [people, setPeople] = useState<Person[] | null>(null);
  const [resetting, setResetting] = useState<string | null>(null);
  const [temp, setTemp] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("People")}
      </h3>
      {people !== null && (
        <Table caption={t("People")} dense>
          <thead>
            <tr>
              <th scope="col">{t("Person")}</th>
              <th scope="col">{t("Role")}</th>
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
              <tr key={p.name}>
                <td>{p.name}</td>
                <td>{p.admin ? t("Administrator") : t("Member")}</td>
                <td>{p.mfa_enabled ? t("On") : t("Off")}</td>
                <td>{p.sessions}</td>
                <td>
                  {p.disabled ? (
                    <Badge tone="danger">{t("Disabled")}</Badge>
                  ) : p.must_change_password ? (
                    <Badge tone="warning">{t("Must choose a password")}</Badge>
                  ) : (
                    <Badge tone="success">{t("Active")}</Badge>
                  )}
                </td>
                <td>
                  <div className="actionrow">
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
    </section>
  );
}

/**
 * The only screen of a session whose password an administrator reset: choose a new one, then
 * carry on. Everything else answers 403 until then.
 */
export function PasswordGate({
  token,
  user,
  onDone,
  onSignOut,
}: {
  token: string;
  user: string;
  onDone: () => void;
  onSignOut: () => void;
}) {
  const [toast, setToast] = useState<string | null>(null);
  // A person with two-factor on also needs a code to change the password.
  const [mfaOn, setMfaOn] = useState(false);
  useEffect(() => {
    let live = true;
    void createTwoFactorApi("/api/auth", token)
      .status()
      .then((r) => live && r.ok && setMfaOn(r.value.enabled));
    return () => {
      live = false;
    };
  }, [token]);
  return (
    <main className="gate">
      <div className="gate-card panel">
        <header className="gate-head">
          <Logo variant="full" size={26} title="obop" />
          <span className="hint">{user}</span>
        </header>
        <Notice tone="warning" title={t("Choose a new password to continue")}>
          {t(
            "An administrator gave you a temporary password. Enter it as the current password and choose your own. Until then you cannot use anything else.",
          )}
        </Notice>
        <PasswordSection
          token={token}
          twoFactor={mfaOn}
          notify={(text) => setToast(text)}
          onDone={onDone}
        />
        {toast && (
          <p className="hint" role="status">
            <Icon name="check" size={14} /> {toast}
          </p>
        )}
        <div className="actionrow">
          <Button variant="ghost" icon="log-out" onClick={onSignOut}>
            {t("Sign out")}
          </Button>
        </div>
      </div>
    </main>
  );
}

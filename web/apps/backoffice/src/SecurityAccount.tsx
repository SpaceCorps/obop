import { useCallback, useEffect, useId, useState } from "react";
import { Badge, Button, Input, Notice, Table, createTwoFactorApi, fmt, t } from "@obop/ui";
import { Icon, Logo } from "./ui.tsx";
import { passkeyMessage } from "./Passkeys.tsx";
import { proveWithPasskey } from "./passkeys.ts";
import { call, search, type Row } from "./api.ts";

export type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/* B7: the account side of Customize > Security: change your password, see and end your
   sessions, and the connected apps. The administrators' People panel is People.tsx. Everything
   goes through the account routes under /api/auth. */

export type Failure = { status: number; code: string; message: string };
type Outcome<T> = { ok: true; value: T } | ({ ok: false } & Failure);

export async function account<T>(
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
export function reason(f: Failure): string {
  if (f.code === "invalid_password") return t("The current password is not right.");
  if (f.code === "invalid_code") return t("That code is not valid.");
  if (f.status === 429) return t("Too many attempts. Wait a minute and try again.");
  if (f.status === 0) return t("The server could not be reached.");
  if (f.status === 404) return t("You are not allowed to do this.");
  if (f.status === 401) return t("You are signed out. Sign in again.");
  return "";
}

export const MIN_PASSWORD = 12;

/** Change your own password. Your other sessions are signed out. */
export function PasswordSection({
  token,
  twoFactor,
  passkeys = false,
  authenticatorApp = true,
  notify,
  onDone,
  title,
}: {
  token: string;
  /** A second factor is on: a code, a recovery code or a passkey is needed too. */
  twoFactor: boolean;
  /** The person has a passkey, which can stand in for the code (D65). */
  passkeys?: boolean;
  /** The person has an authenticator app; without one the code field takes a recovery code only. */
  authenticatorApp?: boolean;
  notify: Notify;
  onDone?: () => void;
  title?: string;
}) {
  const id = useId();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [code, setCode] = useState("");
  const [proved, setProved] = useState(false);
  const [proving, setProving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mismatch = again !== "" && again !== next;
  const tooShort = next !== "" && [...next].length < MIN_PASSWORD;
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {title ?? t("Your password")}
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
            ...(twoFactor && !proved
              ? factor.length > 8
                ? { recovery_code: factor }
                : { code: factor }
              : {}),
          });
          setBusy(false);
          setProved(false);
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
        {twoFactor && !proved && (
          <Input
            label={authenticatorApp ? t("Authenticator code") : t("Recovery code")}
            hint={
              authenticatorApp
                ? t("From your authenticator app, or one of your recovery codes.")
                : t("One of your recovery codes, or use a passkey below.")
            }
            name="code"
            inputMode={authenticatorApp ? "numeric" : undefined}
            autoComplete="one-time-code"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            required={!passkeys}
          />
        )}
        {twoFactor && passkeys && !proved && (
          <div className="actionrow">
            <Button
              icon="key"
              loading={proving}
              onClick={async () => {
                setProving(true);
                setError(null);
                const p = await proveWithPasskey(token);
                setProving(false);
                if (p.ok) setProved(true);
                else setError(passkeyMessage(p));
              }}
            >
              {t("Confirm with a passkey")}
            </Button>
          </div>
        )}
        {proved && (
          <p className="hint pk-confirmed" role="status">
            <Icon name="check-circle" size={14} /> {t("Passkey confirmed.")}
          </p>
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

type GrantRow = {
  id: string;
  client_id: string;
  client_name: string;
  redirect_host: string;
  scopes: string[];
  created_at: string | null;
  expires_at: string | null;
  last_used_at: string | null;
};

/**
 * The apps that connected to this instance through the sign-in flow of MCP clients (D64), each
 * with the groups it was handed and an end date. Revoking one ends its tokens at once.
 */
export function ConnectedAppsSection({ token, notify }: { token: string; notify: Notify }) {
  const id = useId();
  const [rows, setRows] = useState<GrantRow[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(async () => {
    const r = await account<{ grants: GrantRow[] }>(token, "GET", "/oauth/grants");
    setRows(r.ok ? r.value.grants : []);
  }, [token]);
  useEffect(() => void load(), [load]);
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Connected apps")}
      </h3>
      <p className="hint">
        {t(
          "Apps such as Claude Desktop, claude.ai and Claude Code that you signed in for. Each acts as you, with only the groups you handed over, until it ends or you revoke it. Steps that need approval still wait for a person.",
        )}
      </p>
      {rows !== null && rows.length === 0 && (
        <p className="hint">
          {t(
            "No apps are connected. To connect one, add the address of this instance followed by /mcp as a custom connector in the app, then sign in when the browser opens.",
          )}
        </p>
      )}
      {rows !== null && rows.length > 0 && (
        <Table caption={t("Connected apps")} dense>
          <thead>
            <tr>
              <th scope="col">{t("App")}</th>
              <th scope="col">{t("Can act with")}</th>
              <th scope="col">{t("Connected")}</th>
              <th scope="col">{t("Last active")}</th>
              <th scope="col">{t("Ends")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((g) => (
              <tr key={g.id}>
                <td>
                  <strong>{g.client_name}</strong>
                  <div className="hint">{g.redirect_host}</div>
                </td>
                <td>{g.scopes.join(", ")}</td>
                <td>{g.created_at ? fmt.dateTime(g.created_at) : ""}</td>
                <td>{g.last_used_at ? fmt.dateTime(g.last_used_at) : ""}</td>
                <td>{g.expires_at ? fmt.dateTime(g.expires_at) : ""}</td>
                <td>
                  <Button
                    variant="danger"
                    size="sm"
                    loading={busy === g.id}
                    aria-label={t("Revoke {name}", { name: g.client_name })}
                    onClick={async () => {
                      setBusy(g.id);
                      const r = await account<{ revoked: boolean }>(
                        token,
                        "POST",
                        `/oauth/grants/${encodeURIComponent(g.id)}/revoke`,
                        {},
                      );
                      setBusy(null);
                      if (r.ok) {
                        notify(t("{name} can no longer act for you.", { name: g.client_name }));
                        void load();
                      } else notify(reason(r) || t("Could not revoke the app."), "error");
                    }}
                  >
                    {t("Revoke")}
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </section>
  );
}

/**
 * The phones and tablets that a mobile app of this instance is signed in on (the `device` model),
 * with a way to rename each and to end it. A person sees their own; the device administrators see
 * everybody's, and a column then says whose each is. Revoking a device ends its sign-in at once:
 * the app has to sign in again through the system browser.
 */
export function DevicesSection({
  token,
  notify,
  isAdmin = false,
}: {
  token: string;
  notify: Notify;
  isAdmin?: boolean;
}) {
  const id = useId();
  const [rows, setRows] = useState<Row[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const load = useCallback(async () => {
    const r = await search(token, "device", {
      order: [{ field: "registered_at", dir: "desc" }],
      limit: 200,
    });
    // An instance without a sign-in server (--dev-auth) has no devices and no such model.
    setRows("error" in r ? null : r.rows);
  }, [token]);
  useEffect(() => void load(), [load]);
  if (rows === null) return null;
  const showUser = isAdmin || new Set(rows.map((r) => String(r.user))).size > 1;
  const text = (r: Row, k: string) => (typeof r[k] === "string" ? (r[k] as string) : "");
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Devices")}
      </h3>
      <p className="hint">
        {t(
          "Phones and tablets that a mobile app is signed in on. Revoking one signs the app out at once, and it has to sign in again. Changing your password or turning off a second factor signs all of them out.",
        )}
      </p>
      {rows.length === 0 ? (
        <p className="hint">{t("No device is signed in.")}</p>
      ) : (
        <Table caption={t("Devices")} dense>
          <thead>
            <tr>
              <th scope="col">{t("Device")}</th>
              {showUser && <th scope="col">{t("Person")}</th>}
              <th scope="col">{t("App")}</th>
              <th scope="col">{t("Signed in")}</th>
              <th scope="col">{t("Last active")}</th>
              <th scope="col">{t("Ends")}</th>
              <th scope="col">{t("Status")}</th>
              <th scope="col">
                <span className="sr-only">{t("Actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((d) => {
              const active = d.state === "active";
              const editing = renaming?.id === d.id ? renaming : null;
              return (
                <tr key={d.id}>
                  <td>
                    {editing ? (
                      <form
                        className="actionrow"
                        onSubmit={async (e) => {
                          e.preventDefault();
                          setBusy(d.id);
                          const r = await call(token, "device.update", {
                            id: d.id,
                            name: editing.name,
                          });
                          setBusy(null);
                          if (r.kind === "ok") {
                            setRenaming(null);
                            notify(t("Device renamed."));
                            void load();
                          } else notify(t("Could not rename the device."), "error");
                        }}
                      >
                        <Input
                          label={t("Device name")}
                          value={editing.name}
                          maxLength={80}
                          onChange={(e) => setRenaming({ id: d.id, name: e.target.value })}
                          required
                        />
                        <Button type="submit" size="sm" variant="primary" loading={busy === d.id}>
                          {t("Save")}
                        </Button>
                        <Button type="button" size="sm" onClick={() => setRenaming(null)}>
                          {t("Cancel")}
                        </Button>
                      </form>
                    ) : (
                      <>
                        <strong>{text(d, "name")}</strong>
                        <div className="hint">
                          {[text(d, "platform"), text(d, "app_version")].filter(Boolean).join(" ")}
                        </div>
                      </>
                    )}
                  </td>
                  {showUser && <td>{text(d, "user")}</td>}
                  <td>{text(d, "client_name")}</td>
                  <td>{text(d, "registered_at") ? fmt.dateTime(text(d, "registered_at")) : ""}</td>
                  <td>{text(d, "last_seen") ? fmt.dateTime(text(d, "last_seen")) : ""}</td>
                  <td>
                    {active
                      ? text(d, "expires_at")
                        ? fmt.dateTime(text(d, "expires_at"))
                        : ""
                      : text(d, "revoked_at")
                        ? fmt.dateTime(text(d, "revoked_at"))
                        : ""}
                  </td>
                  <td>
                    <Badge tone={active ? "success" : "neutral"}>
                      {active ? t("Signed in") : t("Revoked")}
                    </Badge>
                  </td>
                  <td>
                    {active && !editing && (
                      <div className="actionrow">
                        <Button
                          size="sm"
                          aria-label={t("Rename {name}", { name: text(d, "name") })}
                          onClick={() => setRenaming({ id: d.id, name: text(d, "name") })}
                        >
                          {t("Rename")}
                        </Button>
                        <Button
                          variant="danger"
                          size="sm"
                          loading={busy === d.id}
                          aria-label={t("Revoke {name}", { name: text(d, "name") })}
                          onClick={async () => {
                            setBusy(d.id);
                            const r = await call(token, "device.revoke", { id: d.id });
                            setBusy(null);
                            if (r.kind === "ok") {
                              notify(t("{name} is signed out.", { name: text(d, "name") }));
                              void load();
                            } else notify(t("Could not revoke the device."), "error");
                          }}
                        >
                          {t("Revoke")}
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </Table>
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

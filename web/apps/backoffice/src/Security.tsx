import { useCallback, useEffect, useId, useMemo, useState } from "react";
import {
  Button,
  Input,
  Notice,
  Switch,
  Table,
  TwoFactorPanel,
  createTwoFactorApi,
  fmt,
  msg,
  t,
  twoFactorMessage,
  type TwoFactorStatus,
} from "@obop/ui";
import { search, saveSettings, type Row, type Settings } from "./api.ts";
import { Icon, Logo } from "./ui.tsx";
import { PasswordSection, PeopleSection, SessionsSection } from "./SecurityAccount.tsx";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** Event kinds of the security log (the `security_event` model), in plain words. */
const EVENT_KINDS: Record<string, ReturnType<typeof msg>> = {
  enrol_begin: msg("Setup started"),
  enrol_confirm: msg("Two-factor turned on"),
  disable: msg("Two-factor turned off"),
  reset: msg("Two-factor reset"),
  recovery_regenerate: msg("New recovery codes"),
  recovery_used: msg("Recovery code used"),
  code_failed: msg("Wrong code"),
  lockout: msg("Locked out"),
  limited_session: msg("Signed in without a second factor"),
  login_refused: msg("Sign-in refused"),
  password_change: msg("Password changed"),
  password_change_failed: msg("Wrong password when changing it"),
  password_reset: msg("Password reset by an administrator"),
  sessions_revoked: msg("Sessions signed out"),
  user_disable: msg("Account disabled"),
  user_enable: msg("Account enabled"),
  settings_change: msg("Security setting changed"),
};

/**
 * Customize > Security: your own two-factor authentication for everyone; for administrators the
 * sign-in policy, the reset of another person's second factor and the security log (D53).
 */
export function SecurityTab({
  token,
  settings,
  isAdmin,
  onSettings,
  notify,
  onSignOut,
}: {
  token: string;
  settings: Settings;
  isAdmin: boolean;
  onSettings: (s: Settings) => void;
  notify: Notify;
  onSignOut?: () => void;
}) {
  const api = useMemo(() => createTwoFactorApi("/api/auth", token), [token]);
  const [policy, setPolicy] = useState<TwoFactorStatus | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  useEffect(() => {
    let live = true;
    void api.status().then((r) => live && r.ok && setPolicy(r.value));
    return () => {
      live = false;
    };
  }, [api, refreshKey]);
  return (
    <div className="stack gap-lg security-tab">
      <TwoFactorPanel api={api} notify={notify} onChanged={() => setRefreshKey((k) => k + 1)} />
      <PasswordSection token={token} twoFactor={!!policy?.enabled} notify={notify} />
      <SessionsSection
        token={token}
        notify={notify}
        onSignOut={onSignOut ?? (() => location.reload())}
      />
      {isAdmin && (
        <>
          <PeopleSection token={token} notify={notify} />
          <PolicySection
            token={token}
            settings={settings}
            status={policy}
            onSettings={onSettings}
            notify={notify}
          />
          <ResetSection token={token} notify={notify} onReset={() => setRefreshKey((k) => k + 1)} />
          <EventLog token={token} refreshKey={refreshKey} />
        </>
      )}
    </div>
  );
}

function PolicySection({
  token,
  settings,
  status,
  onSettings,
  notify,
}: {
  token: string;
  settings: Settings;
  status: TwoFactorStatus | null;
  onSettings: (s: Settings) => void;
  notify: Notify;
}) {
  const id = useId();
  const admins = settings.security?.require_mfa_for_admins ?? status?.require_admins ?? true;
  const everyone = settings.security?.require_mfa_for_all ?? status?.require_all ?? false;
  const [busy, setBusy] = useState(false);
  async function save(patch: { require_mfa_for_admins?: boolean; require_mfa_for_all?: boolean }) {
    setBusy(true);
    const next: Settings = {
      ...settings,
      security: { require_mfa_for_admins: admins, require_mfa_for_all: everyone, ...patch },
    };
    const r = await saveSettings(token, next);
    setBusy(false);
    if (r.kind === "ok") {
      onSettings(next);
      notify(t("Sign-in policy saved"));
    } else notify(t("Could not save"), "error");
  }
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Sign-in policy")}
      </h3>
      <Switch
        label={t("Require 2FA for admins")}
        checked={admins}
        disabled={busy}
        onChange={(e) => void save({ require_mfa_for_admins: e.target.checked })}
      />
      <Switch
        label={t("Require 2FA for everyone")}
        checked={everyone}
        disabled={busy}
        onChange={(e) => void save({ require_mfa_for_all: e.target.checked })}
      />
      <p className="hint">
        {t(
          "Applies the next time someone signs in. A person without two-factor can then only set it up, until they have. Agent tokens are not affected.",
        )}
      </p>
      {status && !status.available && (
        <Notice tone="warning" title={t("Not available on this server")}>
          {t(
            "Two-factor authentication needs a server key to protect the secrets. Ask the operator to set it up.",
          )}
        </Notice>
      )}
    </section>
  );
}

function ResetSection({
  token,
  notify,
  onReset,
}: {
  token: string;
  notify: Notify;
  onReset: () => void;
}) {
  const id = useId();
  const [user, setUser] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Reset someone's two-factor")}
      </h3>
      <form
        className="stack"
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          setError(null);
          const res = await fetch("/api/auth/mfa/reset", {
            method: "POST",
            headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
            body: JSON.stringify({ user }),
          });
          setBusy(false);
          if (res.ok) {
            notify(t("Two-factor reset for {user}", { user: user.trim().toLowerCase() }));
            setUser("");
            onReset();
          } else {
            const body = (await res.json().catch(() => ({}))) as { code?: string };
            setError(
              res.status === 400
                ? t("There is no such person, or they have no two-factor to reset.")
                : twoFactorMessage({ status: res.status, code: body.code ?? "" }),
            );
          }
        }}
      >
        <Input
          label={t("Username")}
          hint={t(
            "For someone who lost their phone and recovery codes. They are signed out and must set it up again. Recorded in the security log.",
          )}
          value={user}
          onChange={(e) => setUser(e.target.value)}
          autoComplete="off"
          error={error ?? undefined}
          required
        />
        <div className="actionrow">
          <Button type="submit" loading={busy} disabled={!user.trim()}>
            {t("Reset two-factor")}
          </Button>
        </div>
      </form>
    </section>
  );
}

function EventLog({ token, refreshKey }: { token: string; refreshKey: number }) {
  const id = useId();
  const [rows, setRows] = useState<Row[] | null>(null);
  const load = useCallback(async () => {
    const r = await search(token, "security_event", {
      order: [{ field: "at", dir: "desc" }],
      limit: 25,
    });
    setRows("error" in r ? [] : r.rows);
  }, [token]);
  useEffect(() => void load(), [load, refreshKey]);
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Security log")}
      </h3>
      {rows === null ? null : rows.length === 0 ? (
        <p className="hint">{t("Nothing has been recorded yet.")}</p>
      ) : (
        <Table caption={t("Security log")} dense>
          <thead>
            <tr>
              <th scope="col">{t("Time")}</th>
              <th scope="col">{t("Person")}</th>
              <th scope="col">{t("Event")}</th>
              <th scope="col">{t("Done by")}</th>
              <th scope="col">{t("Note")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.id}>
                <td>{fmt.dateTime(String(r.at))}</td>
                <td>{String(r.user)}</td>
                <td>
                  {EVENT_KINDS[String(r.kind)] ? t(EVENT_KINDS[String(r.kind)]!) : String(r.kind)}
                </td>
                <td>{String(r.actor)}</td>
                <td>{String(r.detail ?? "")}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </section>
  );
}

/**
 * The only screen of a session that signed in without the second factor an administrator
 * requires (D53): set it up, then carry on. Everything else answers 403 until then.
 */
export function EnrolmentGate({
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
  const api = useMemo(() => createTwoFactorApi("/api/auth", token), [token]);
  const [toast, setToast] = useState<string | null>(null);
  return (
    <main className="gate">
      <div className="gate-card panel">
        <header className="gate-head">
          <Logo variant="full" size={26} title="obop" />
          <span className="hint">{user}</span>
        </header>
        <Notice tone="warning" title={t("Set up two-factor authentication to continue")}>
          {t(
            "Your administrator requires a second step when you sign in. Until you have set it up, you cannot use anything else.",
          )}
        </Notice>
        <TwoFactorPanel api={api} onActive={onDone} notify={(text) => setToast(text)} />
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

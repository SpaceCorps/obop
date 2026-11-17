import { useCallback, useEffect, useId, useMemo, useState } from "react";
import {
  Button,
  InstanceLogo,
  Notice,
  Switch,
  Table,
  TwoFactorPanel,
  createTwoFactorApi,
  fmt,
  t,
  type TwoFactorStatus,
} from "@obop/ui";
import { search, saveSettings, type Row, type Settings } from "./api.ts";
import { Icon } from "./ui.tsx";
import { AgentTokensSection } from "./AgentTokens.tsx";
import { PasskeysSection } from "./Passkeys.tsx";
import {
  ConnectedAppsSection,
  DevicesSection,
  PasswordSection,
  SessionsSection,
} from "./SecurityAccount.tsx";
import type { App } from "./Chrome.tsx";
import { PeopleSection } from "./People.tsx";
import { MicrosoftSignInSection } from "./SsoAdmin.tsx";
import { EVENT_KINDS } from "./securityEvents.ts";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/**
 * Customize > Security: your own two-factor authentication for everyone; for administrators the
 * sign-in policy, the security log (a person's second factor is reset from their row in People) (D53).
 */
export function SecurityTab({
  token,
  settings,
  isAdmin,
  me,
  apps,
  onSettings,
  notify,
  onSignOut,
}: {
  token: string;
  settings: Settings;
  isAdmin: boolean;
  /** Who is signed in (the name the sign-in gave), for what is shown as theirs. */
  me?: string;
  /** The launcher apps in their order: People names its access rows after them. */
  apps?: App[];
  onSettings: (s: Settings) => void;
  notify: Notify;
  onSignOut?: () => void;
}) {
  const api = useMemo(() => createTwoFactorApi("/api/auth", token), [token]);
  const [policy, setPolicy] = useState<TwoFactorStatus | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  // Bumped when Microsoft sign-in changes, so that People shows or hides its column.
  const [ssoKey, setSsoKey] = useState(0);
  useEffect(() => {
    let live = true;
    void api.status().then((r) => live && r.ok && setPolicy(r.value));
    return () => {
      live = false;
    };
  }, [api, refreshKey]);
  // "Sign out everywhere" and the end of an own session by the server leave by the same door.
  const signedOut = onSignOut ?? (() => location.reload());
  return (
    <div className="stack gap-lg security-tab">
      <TwoFactorPanel api={api} notify={notify} onChanged={() => setRefreshKey((k) => k + 1)} />
      <PasskeysSection
        token={token}
        notify={notify}
        onChanged={() => setRefreshKey((k) => k + 1)}
      />
      <PasswordSection
        token={token}
        twoFactor={!!(policy?.second_factor ?? policy?.enabled)}
        passkeys={(policy?.passkeys ?? 0) > 0}
        authenticatorApp={!!policy?.enabled}
        notify={notify}
      />
      <SessionsSection token={token} notify={notify} onSignOut={signedOut} />
      <ConnectedAppsSection token={token} notify={notify} />
      <AgentTokensSection token={token} notify={notify} />
      <DevicesSection token={token} notify={notify} isAdmin={isAdmin} me={me} />
      {isAdmin && (
        <>
          <PeopleSection
            token={token}
            notify={notify}
            refreshKey={ssoKey}
            me={me}
            apps={apps}
            onSignedOut={signedOut}
            onTwoFactorReset={() => setRefreshKey((k) => k + 1)}
          />
          <MicrosoftSignInSection
            token={token}
            notify={notify}
            refreshKey={ssoKey}
            onChanged={() => setSsoKey((k) => k + 1)}
          />
          <PolicySection
            token={token}
            settings={settings}
            status={policy}
            onSettings={onSettings}
            notify={notify}
          />
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
        <Table caption={t("Security log")} dense stack>
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
                <td data-label={t("Time")}>{fmt.dateTime(String(r.at))}</td>
                <td data-label={t("Person")}>{String(r.user)}</td>
                <td className="cell-main">
                  {EVENT_KINDS[String(r.kind)] ? t(EVENT_KINDS[String(r.kind)]!) : String(r.kind)}
                </td>
                <td data-label={t("Done by")}>{String(r.actor)}</td>
                <td data-label={t("Note")}>{String(r.detail ?? "")}</td>
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
          <InstanceLogo variant="full" size={26} />
          <span className="hint">{user}</span>
        </header>
        <Notice tone="warning" title={t("Set up two-factor authentication to continue")}>
          {t(
            "Your administrator requires a second step when you sign in. Until you have set it up, you cannot use anything else.",
          )}
        </Notice>
        <TwoFactorPanel api={api} onActive={onDone} notify={(text) => setToast(text)} />
        <PasskeysSection token={token} onAdded={onDone} notify={(text) => setToast(text)} />
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

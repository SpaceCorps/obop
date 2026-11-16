import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Badge, Button, ConfirmDialog, Dialog, Input, Notice, Switch, t } from "@obop/ui";
import { call } from "./api.ts";
import type { Notify } from "./SecurityAccount.tsx";
import { copyText } from "./signLinks.ts";

/* Customize > Security: "Sign in with Microsoft" for administrators (D91). One Microsoft Entra
   tenant, set up, switched, changed and removed with the commands of the model `sso_provider`
   (`list`, `configure`, `clear`). Binding a person to a Microsoft identity is a dialog of People
   (`PeopleSso.tsx`). Nothing here keeps the client secret: it is in the state of the form while
   the form is open, goes to the server once and is never read back (the server answers
   `secret_set` and nothing more). */

/** What the server says about the one provider (`sso_provider.list`). */
export type SsoProvider = {
  tenant: string;
  client_id: string;
  enabled: boolean;
  secret_set: boolean;
  /** The administrators who still sign in with a password and a second step, separated by commas. */
  break_glass: string;
  /** Whether there are at least two of them, which Microsoft only for an account needs. */
  break_glass_enough: boolean;
};

export type SsoState =
  | { kind: "loading" }
  /** No provider is stored. */
  | { kind: "none" }
  | { kind: "set"; provider: SsoProvider }
  /** The instance cannot say (an old server without the model, or a failure). */
  | { kind: "unknown"; absent: boolean };

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** Whether `s` is a GUID the way the server reads one (a tenant, an application or an object id). */
export const isGuid = (s: string) => GUID.test(s.trim());

/** The address that Microsoft sends the person back to; it is registered in the app registration. */
export const redirectAddress = () => `${location.origin}/api/auth/oidc/callback`;

/** The provider of this instance, or what stops us from knowing it. */
export async function readSso(token: string): Promise<SsoState> {
  const r = await call(token, "sso_provider.list", {});
  if (r.kind !== "ok") return { kind: "unknown", absent: r.kind === "error" && r.status === 404 };
  const rows = Array.isArray(r.value)
    ? r.value
    : ((r.value as { rows?: unknown[] } | null)?.rows ?? []);
  const row = rows[0] as Partial<SsoProvider> | undefined;
  if (!row) return { kind: "none" };
  return {
    kind: "set",
    provider: {
      tenant: String(row.tenant ?? ""),
      client_id: String(row.client_id ?? ""),
      enabled: row.enabled === true,
      secret_set: row.secret_set === true,
      break_glass: String(row.break_glass ?? ""),
      break_glass_enough: row.break_glass_enough === true,
    },
  };
}

/** The server's refusal as it words it, or a plain sentence when it said nothing. */
function refusal(r: { kind: string; message?: string }, fallback: string): string {
  return (r.kind === "error" && r.message) || fallback;
}

/** The set-up and change form. */
function SsoFormDialog({
  token,
  provider,
  onClose,
  onSaved,
}: {
  token: string;
  provider: SsoProvider | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [tenant, setTenant] = useState(provider?.tenant ?? "");
  const [clientId, setClientId] = useState(provider?.client_id ?? "");
  const [secret, setSecret] = useState("");
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const address = useRef<HTMLInputElement>(null);
  const [copied, setCopied] = useState<"ok" | "failed" | null>(null);
  const stored = provider?.secret_set === true;

  const tenantError = asked && !isGuid(tenant) ? t("The tenant ID is a GUID.") : undefined;
  const clientError = asked && !isGuid(clientId) ? t("The application ID is a GUID.") : undefined;
  const secretError =
    asked && !stored && secret.trim() === "" ? t("Enter the client secret.") : undefined;

  async function save() {
    setAsked(true);
    if (!isGuid(tenant) || !isGuid(clientId) || (!stored && secret.trim() === "")) return;
    setBusy(true);
    setError(null);
    const r = await call(token, "sso_provider.configure", {
      tenant: tenant.trim(),
      client_id: clientId.trim(),
      // Left empty on a change: the server keeps the sealed secret it has.
      ...(secret.trim() === "" ? {} : { client_secret: secret.trim() }),
    });
    setBusy(false);
    if (r.kind === "ok") {
      setSecret("");
      onSaved();
      onClose();
    } else setError(refusal(r, t("Could not save the Microsoft sign-in.")));
  }

  async function copy() {
    setCopied((await copyText(redirectAddress(), address.current)) ? "ok" : "failed");
  }

  return (
    <Dialog
      title={provider ? t("Change Microsoft sign-in") : t("Set up Microsoft sign-in")}
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void save()}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <form
        className="stack"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <Input
          ref={address}
          label={t("Redirect address")}
          hint={t(
            "Register this as a Web redirect address in the app registration at Microsoft. It must be the address people type to reach this instance.",
          )}
          value={redirectAddress()}
          readOnly
          spellCheck={false}
          className="sso-address"
        />
        <div className="actionrow">
          <Button onClick={() => void copy()}>{t("Copy")}</Button>
          {copied && (
            <span className="hint" role="status">
              {copied === "ok" ? t("Copied") : t("Could not copy. Select the address and copy it.")}
            </span>
          )}
        </div>
        <Input
          label={t("Tenant ID")}
          hint={t("The directory (tenant) ID of your organisation at Microsoft, a GUID.")}
          value={tenant}
          onChange={(e) => setTenant(e.target.value)}
          error={tenantError}
          autoComplete="off"
          spellCheck={false}
          required
        />
        <Input
          label={t("Application (client) ID")}
          hint={t("The ID of the app registration, a GUID.")}
          value={clientId}
          onChange={(e) => setClientId(e.target.value)}
          error={clientError}
          autoComplete="off"
          spellCheck={false}
          required
        />
        <Input
          label={t("Client secret")}
          hint={
            stored
              ? t("Leave empty to keep the stored secret.")
              : t(
                  "The secret's Value, not its Secret ID. It is stored sealed and never shown again.",
                )
          }
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          error={secretError}
          autoComplete="off"
          spellCheck={false}
          required={!stored}
        />
        {error && (
          <div className="confirm-error" role="alert">
            {error}
          </div>
        )}
      </form>
    </Dialog>
  );
}

/** Administrators: set up Sign in with Microsoft. Shown right after People. */
export function MicrosoftSignInSection({
  token,
  notify,
  onChanged,
  refreshKey = 0,
}: {
  token: string;
  notify: Notify;
  /** Called after a change, so that People shows the column when there is something to show. */
  onChanged?: () => void;
  refreshKey?: number;
}) {
  const id = useId();
  const [state, setState] = useState<SsoState>({ kind: "loading" });
  const [editing, setEditing] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [busy, setBusy] = useState(false);
  const [removeError, setRemoveError] = useState<string | null>(null);

  const load = useCallback(async () => setState(await readSso(token)), [token]);
  useEffect(() => void load(), [load, refreshKey]);

  if (state.kind === "loading" || (state.kind === "unknown" && state.absent)) return null;
  const provider = state.kind === "set" ? state.provider : null;

  async function show(on: boolean) {
    if (!provider) return;
    setBusy(true);
    const r = await call(token, "sso_provider.configure", {
      tenant: provider.tenant,
      client_id: provider.client_id,
      enabled: on,
    });
    setBusy(false);
    if (r.kind === "ok")
      notify(on ? t("Microsoft sign-in is on.") : t("Microsoft sign-in is off."));
    else notify(refusal(r, t("Could not save the Microsoft sign-in.")), "error");
    await load();
    if (r.kind === "ok") onChanged?.();
  }

  async function remove() {
    setBusy(true);
    setRemoveError(null);
    const r = await call(token, "sso_provider.clear", {});
    setBusy(false);
    if (r.kind === "ok") {
      setRemoving(false);
      notify(t("Microsoft sign-in removed."));
      await load();
      onChanged?.();
    } else setRemoveError(refusal(r, t("Could not remove the Microsoft sign-in.")));
  }

  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <div className="people-head">
        <h3 id={`${id}-h`} className="overline">
          {t("Sign in with Microsoft")}
        </h3>
        {state.kind !== "unknown" && (
          <Badge tone={!provider ? "neutral" : provider.enabled ? "success" : "warning"}>
            {!provider ? t("Not set up") : provider.enabled ? t("On") : t("Off")}
          </Badge>
        )}
      </div>
      <p className="hint">
        {t(
          "People sign in with their Microsoft work account. It creates no account and gives no permission: bind each person under People.",
        )}
      </p>
      {state.kind === "unknown" && (
        <Notice tone="warning" title={t("Sign in with Microsoft")}>
          {t("Microsoft sign-in could not be loaded. Try again.")}{" "}
          <Button size="sm" onClick={() => void load()}>
            {t("Try again")}
          </Button>
        </Notice>
      )}
      {state.kind === "none" && (
        <div className="actionrow">
          <Button variant="primary" onClick={() => setEditing(true)}>
            {t("Set up Microsoft sign-in")}
          </Button>
        </div>
      )}
      {provider && (
        <>
          <dl className="sso-summary">
            <div>
              <dt>{t("Tenant ID")}</dt>
              <dd>
                <code>{provider.tenant}</code>
              </dd>
            </div>
            <div>
              <dt>{t("Application (client) ID")}</dt>
              <dd>
                <code>{provider.client_id}</code>
              </dd>
            </div>
            <div>
              <dt>{t("Client secret")}</dt>
              <dd>{provider.secret_set ? t("Stored") : t("Not stored")}</dd>
            </div>
          </dl>
          <Switch
            label={t("Show the button on the sign-in page")}
            checked={provider.enabled}
            disabled={busy}
            onChange={(e) => void show(e.target.checked)}
          />
          <div className="actionrow">
            <Button variant="ghost" onClick={() => setEditing(true)}>
              {t("Change")}
            </Button>
            <Button
              variant="ghost"
              className="danger-text"
              onClick={() => {
                setRemoveError(null);
                setRemoving(true);
              }}
            >
              {t("Remove")}
            </Button>
          </div>
          {provider.break_glass_enough ? (
            <p className="hint">
              {t(
                "Administrators who can still sign in with a password and a second step: {names}.",
                { names: provider.break_glass },
              )}
            </p>
          ) : (
            <Notice tone="warning" title={t("Sign in with Microsoft")}>
              {t(
                "At least two other administrators with a password and a second step are needed before an account can be made Microsoft only.",
              )}
            </Notice>
          )}
        </>
      )}
      {editing && (
        <SsoFormDialog
          token={token}
          provider={provider}
          onClose={() => setEditing(false)}
          onSaved={() => {
            notify(t("Microsoft sign-in saved."));
            void load();
            onChanged?.();
          }}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={t("Remove Microsoft sign-in?")}
          confirmLabel={t("Remove")}
          busy={busy}
          error={removeError}
          onConfirm={() => void remove()}
          onClose={() => setRemoving(false)}
        >
          <p>
            {t(
              "The button leaves the sign-in page. Bindings stay. People who sign in with Microsoft only cannot sign in until it is set up again.",
            )}
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}

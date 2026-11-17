import { useCallback, useEffect, useId, useRef, useState, type FocusEvent } from "react";
import { Badge, Button, ConfirmDialog, Dialog, Input, Notice, Switch, Textarea, t } from "@obop/ui";
import { call } from "./api.ts";
import { account, type Notify } from "./SecurityAccount.tsx";
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
export const redirectAddress = (origin: string = location.origin) =>
  `${origin}/api/auth/oidc/callback`;

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

/** How many accounts sign in with Microsoft only, or null when the list could not be read. */
export async function countSsoOnly(token: string): Promise<number | null> {
  const r = await account<{ users: { sso_only?: boolean }[] }>(token, "GET", "/users");
  return r.ok ? r.value.users.filter((u) => u.sso_only === true).length : null;
}

/** "3 people sign in with Microsoft only." */
export const onlyPeople = (count: number) =>
  t(
    "{count, plural, one {# person signs in with Microsoft only.} other {# people sign in with Microsoft only.}}",
    { count },
  );

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
  // A field the person has left with something typed in it is judged at once, not only at Save.
  const [left, setLeft] = useState({ tenant: false, client: false });
  const [attempt, setAttempt] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const address = useRef<HTMLTextAreaElement>(null);
  const addressHint = useId();
  const form = useRef<HTMLFormElement>(null);
  // A second Enter in the same moment must not send a second request.
  const inflight = useRef(false);
  const [copied, setCopied] = useState<"ok" | "failed" | null>(null);
  const stored = provider?.secret_set === true;

  const tenantError =
    (asked || left.tenant) && !isGuid(tenant)
      ? t("This is not a tenant ID: it has 32 letters and digits in five groups.")
      : undefined;
  const clientError =
    (asked || left.client) && !isGuid(clientId)
      ? t("This is not an application ID: it has 32 letters and digits in five groups.")
      : undefined;
  const secretError =
    asked && !stored && secret.trim() === "" ? t("Enter the client secret.") : undefined;
  // After a refused Save the keyboard goes to the first field that is wrong, and the reader hears it.
  useEffect(() => {
    if (attempt > 0) form.current?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }, [attempt]);
  // The "Copied" line is cleared after a few seconds, so the next copy is announced again.
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(null), 4000);
    return () => clearTimeout(timer);
  }, [copied]);
  /** Judge a field when the person leaves it, unless they go to a button (Save, Cancel). */
  const leave = (key: "tenant" | "client", value: string) => (e: FocusEvent<HTMLInputElement>) => {
    if (value.trim() !== "" && !(e.relatedTarget instanceof HTMLButtonElement))
      setLeft((l) => ({ ...l, [key]: true }));
  };

  async function save() {
    if (inflight.current) return;
    setAsked(true);
    if (!isGuid(tenant) || !isGuid(clientId) || (!stored && secret.trim() === "")) {
      setAttempt((a) => a + 1);
      return;
    }
    inflight.current = true;
    setBusy(true);
    setError(null);
    const r = await call(token, "sso_provider.configure", {
      tenant: tenant.trim(),
      client_id: clientId.trim(),
      // Left empty on a change: the server keeps the sealed secret it has.
      ...(secret.trim() === "" ? {} : { client_secret: secret.trim() }),
    });
    inflight.current = false;
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
      // While the request is in flight nothing closes it, or a refusal would be lost.
      onClose={busy ? () => {} : onClose}
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
        ref={form}
        className="stack"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <div className="sso-address">
          {/* The address is a text area so that a long one wraps and is read in full on a phone. */}
          <div className="sso-address-row">
            <Textarea
              ref={address}
              label={t("Redirect address")}
              aria-describedby={addressHint}
              value={redirectAddress()}
              readOnly
              rows={2}
              spellCheck={false}
            />
            <Button aria-label={t("Copy redirect address")} onClick={() => void copy()}>
              {t("Copy")}
            </Button>
          </div>
          <p className="hint" id={addressHint}>
            {`${t(
              "Register this as a Web redirect address in the app registration at Microsoft. It must be the address people type to reach this instance.",
            )} ${t(
              "This is the address of the page you are on. If sign-in later fails with a redirect address mismatch, ask whoever runs the server to set its public address.",
            )}`}
          </p>
          {/* Always in the page and filled afterwards, so that a reader announces the change. */}
          <span className="hint" role="status">
            {copied === "ok"
              ? t("Copied")
              : copied === "failed"
                ? t("Could not copy. Select the address and copy it.")
                : ""}
          </span>
        </div>
        <Input
          label={t("Tenant ID")}
          hint={t(
            "Copy it from the Overview page of the app registration at Microsoft (Directory (tenant) ID). It looks like 00000000-0000-0000-0000-000000000000.",
          )}
          value={tenant}
          onChange={(e) => setTenant(e.target.value)}
          onBlur={leave("tenant", tenant)}
          error={tenantError}
          autoComplete="off"
          spellCheck={false}
          required
        />
        <Input
          label={t("Application (client) ID")}
          hint={t(
            "Copy it from the same Overview page (Application (client) ID). It looks like 00000000-0000-0000-0000-000000000000.",
          )}
          value={clientId}
          onChange={(e) => setClientId(e.target.value)}
          onBlur={leave("client", clientId)}
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
                  "The secret's Value, not its Secret ID. It is stored encrypted and never shown again.",
                )
          }
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          error={secretError}
          // "off" is ignored by browsers: a password manager would fill the person's own login
          // password here, and on a change that would replace the stored secret.
          autoComplete="new-password"
          data-1p-ignore
          data-lpignore="true"
          spellCheck={false}
          required={!stored}
        />
        {provider && (
          <p className="hint">
            {t(
              "A wrong tenant, application ID or secret stops every Microsoft sign-in, also for people who sign in with Microsoft only.",
            )}
          </p>
        )}
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
  // How many accounts sign in with Microsoft only (null: not known).
  const [onlyCount, setOnlyCount] = useState<number | null>(null);
  // The question before the switch is turned off; `people` is null when that could not be read.
  const [turningOff, setTurningOff] = useState<{ people: number | null } | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const [focusHeading, setFocusHeading] = useState(0);

  const load = useCallback(async () => {
    const [next, only] = await Promise.all([readSso(token), countSsoOnly(token)]);
    setState(next);
    setOnlyCount(only);
  }, [token]);
  useEffect(() => void load(), [load, refreshKey]);
  // The button that opened a dialog is gone after Set up and Remove: the keyboard goes to the heading.
  useEffect(() => {
    if (focusHeading > 0) heading.current?.focus();
  }, [focusHeading]);

  if (state.kind === "loading" || (state.kind === "unknown" && state.absent)) return null;
  const provider = state.kind === "set" ? state.provider : null;

  async function show(on: boolean) {
    if (!provider) return;
    if (!on) {
      // Turning the button off strands everyone who signs in with Microsoft only: ask first.
      setBusy(true);
      const people = await countSsoOnly(token);
      setBusy(false);
      if (people !== 0) return setTurningOff({ people });
    }
    await apply(on);
  }

  async function apply(on: boolean) {
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
      setFocusHeading((k) => k + 1);
      onChanged?.();
    } else setRemoveError(refusal(r, t("Could not remove the Microsoft sign-in.")));
  }

  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <div className="people-head">
        <h3 id={`${id}-h`} ref={heading} tabIndex={-1} className="overline">
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
          {onlyCount !== null && onlyCount > 0 && <p className="hint">{onlyPeople(onlyCount)}</p>}
          <div className="actionrow sso-actions">
            <Button variant="ghost" onClick={() => setEditing(true)}>
              {t("Change settings")}
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
          {provider.break_glass !== "" && (
            <p className="hint">
              {t(
                "Administrators who can still sign in with a password and a second step: {names}.",
                { names: provider.break_glass },
              )}
            </p>
          )}
        </>
      )}
      {editing && (
        <SsoFormDialog
          token={token}
          provider={provider}
          onClose={() => setEditing(false)}
          onSaved={() => {
            // The first set-up switches the button on for everyone at once, with nobody bound yet.
            notify(
              provider === null
                ? t("Microsoft sign-in saved. The button is on: now bind each person under People.")
                : t("Microsoft sign-in saved."),
            );
            // After a first set-up the button that opened the form is replaced: the heading takes the focus.
            void load().then(() => provider === null && setFocusHeading((k) => k + 1));
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
          <div className="stack">
            <p>
              {t(
                "The button leaves the sign-in page. Bindings stay. People who sign in with Microsoft only cannot sign in until it is set up again.",
              )}
            </p>
            {onlyCount !== null && onlyCount > 0 && <p>{onlyPeople(onlyCount)}</p>}
          </div>
        </ConfirmDialog>
      )}
      {turningOff && (
        <ConfirmDialog
          title={t("Turn off Microsoft sign-in?")}
          confirmLabel={t("Turn off")}
          busy={busy}
          onConfirm={() => void apply(false).then(() => setTurningOff(null))}
          onClose={() => setTurningOff(null)}
        >
          <div className="stack">
            <p>{t("The button leaves the sign-in page.")}</p>
            <p>
              {turningOff.people === null
                ? t("People who sign in with Microsoft only cannot sign in while it is off.")
                : `${onlyPeople(turningOff.people)} ${t("They cannot sign in while it is off, and their password is refused.")}`}
            </p>
          </div>
        </ConfirmDialog>
      )}
    </section>
  );
}

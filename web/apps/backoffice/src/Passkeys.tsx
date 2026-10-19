import { useCallback, useEffect, useId, useState, type FormEvent } from "react";
import { Badge, Button, Checkbox, Input, Notice, Switch, fmt, t } from "@obop/ui";
import { Icon } from "./ui.tsx";
import {
  listPasskeys,
  passkeysSupported,
  proveWithPasskey,
  registerPasskey,
  removePasskey,
  renamePasskey,
  setPasswordless,
  type Failure,
  type Outcome,
  type Passkey,
  type PasskeyList,
  type Reauth,
} from "./passkeys.ts";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** A refusal in the person's language. The server's own sentence is kept for the rules it states. */
export function passkeyMessage(f: Failure): string {
  if (f.kind === "cancelled")
    return t("The passkey request was cancelled or timed out. Try again.");
  if (f.kind === "duplicate") return t("This passkey is already registered here.");
  if (f.kind === "unsupported")
    return t("This browser or device cannot make that kind of passkey.");
  if (f.kind === "network") return t("Could not reach the server.");
  if (f.code === "invalid_password") return t("That password is not right.");
  if (f.status === 429) return t("Too many attempts. Wait a minute and try again.");
  if (f.status === 401) return t("The passkey or code was not accepted. Try again.");
  if (f.status === 404) return t("You are not allowed to do this.");
  // 400 and 503 carry a sentence that says exactly what to change (the policy, the address).
  return f.message || t("Something went wrong. Try again.");
}

type Mode = "code" | "recovery" | "passkey";

/**
 * Re-authentication before a change: the password, and the second factor when one is on (a code,
 * a recovery code, or a passkey). Used by every passkey action and the password form.
 */
export function ReauthForm({
  token,
  hasTotp,
  hasPasskeys,
  heading,
  hint,
  submitLabel,
  tone = "primary",
  children,
  onSubmit,
  onCancel,
}: {
  token: string;
  hasTotp: boolean;
  hasPasskeys: boolean;
  heading: string;
  hint?: string;
  submitLabel: string;
  tone?: "primary" | "danger";
  /** Fields of the action itself, above the confirmation. */
  children?: React.ReactNode;
  /** Resolves to an error text, or null when the action went through. */
  onSubmit: (reauth: Reauth) => Promise<string | null>;
  onCancel: () => void;
}) {
  const secondFactor = hasTotp || hasPasskeys;
  const [mode, setMode] = useState<Mode>(hasTotp ? "code" : "passkey");
  const [proved, setProved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [proving, setProving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function prove() {
    setProving(true);
    setError(null);
    const r = await proveWithPasskey(token);
    setProving(false);
    if (r.ok) setProved(true);
    else setError(passkeyMessage(r));
  }

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    const reauth: Reauth = { password: String(f.get("password") ?? "") };
    const value = String(f.get("code") ?? "").replace(/\s/g, "");
    if (secondFactor && !proved && value) {
      if (mode === "recovery") reauth.recovery_code = value;
      else reauth.code = value;
    }
    setBusy(true);
    setError(null);
    const err = await onSubmit(reauth);
    setBusy(false);
    // The proof is spent whether or not the action went through.
    setProved(false);
    if (err) setError(err);
  }

  const needsCode = secondFactor && !proved && mode !== "passkey";
  return (
    <form className="tf-form" onSubmit={(e) => void submit(e)}>
      <h4>{heading}</h4>
      {hint && <p className="hint">{hint}</p>}
      {children}
      <Input
        label={t("Password")}
        name="password"
        type="password"
        autoComplete="current-password"
        required
      />
      {needsCode && (
        <Input
          key={mode}
          label={mode === "recovery" ? t("Recovery code") : t("6-digit code")}
          name="code"
          inputMode={mode === "recovery" ? undefined : "numeric"}
          autoComplete={mode === "recovery" ? "off" : "one-time-code"}
          spellCheck={false}
          required
        />
      )}
      {secondFactor && mode === "passkey" && !proved && (
        <div className="pk-prove">
          <Button icon="key" loading={proving} onClick={() => void prove()}>
            {t("Confirm with a passkey")}
          </Button>
          <span className="hint">{t("Your second step: touch or unlock the passkey.")}</span>
        </div>
      )}
      {proved && (
        <p className="hint pk-confirmed" role="status">
          <Icon name="check-circle" size={14} /> {t("Passkey confirmed.")}
        </p>
      )}
      {error && <Notice tone="danger" title={error} />}
      <div className="tf-actions">
        <Button
          type="submit"
          variant={tone}
          loading={busy}
          disabled={secondFactor && mode === "passkey" && !proved}
        >
          {submitLabel}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          {t("Cancel")}
        </Button>
        {secondFactor && !proved && (
          <span className="pk-switch">
            {mode !== "passkey" && hasPasskeys && (
              <Button variant="ghost" size="sm" onClick={() => setMode("passkey")}>
                {t("Use a passkey")}
              </Button>
            )}
            {mode !== "code" && hasTotp && (
              <Button variant="ghost" size="sm" onClick={() => setMode("code")}>
                {t("Use an authenticator code")}
              </Button>
            )}
            {mode !== "recovery" && (
              <Button variant="ghost" size="sm" onClick={() => setMode("recovery")}>
                {t("Use a recovery code")}
              </Button>
            )}
          </span>
        )}
      </div>
    </form>
  );
}

type View =
  | { name: "idle" }
  | { name: "add" }
  | { name: "rename"; id: string }
  | { name: "remove"; id: string }
  | { name: "passwordless"; on: boolean }
  | { name: "codes"; codes: string[]; label: string };

/**
 * Customize > Security: your passkeys (D65). A passkey is a second step at sign-in that satisfies
 * the same policy as an authenticator app, and, when you allow it, a way to sign in without a
 * password. Adding, renaming and removing one asks for your password again.
 */
export function PasskeysSection({
  token,
  notify,
  onChanged,
  onAdded,
}: {
  token: string;
  notify: Notify;
  /** Whenever the list or the second factor may have changed. */
  onChanged?: () => void;
  /** After a passkey was added and its recovery codes (if any) were saved. */
  onAdded?: () => void;
}) {
  const id = useId();
  const [list, setList] = useState<PasskeyList | "error" | null>(null);
  const [view, setView] = useState<View>({ name: "idle" });
  const [saved, setSaved] = useState(false);
  const [name, setName] = useState("");
  const supported = passkeysSupported();

  const load = useCallback(async () => {
    const r = await listPasskeys(token);
    setList(r.ok ? r.value : "error");
  }, [token]);
  useEffect(() => void load(), [load]);

  if (list === null)
    return (
      <p className="hint" role="status">
        {t("Loading")}
      </p>
    );
  if (list === "error") return <Notice tone="danger" title={t("Passkeys could not be loaded.")} />;

  const finished = (r: Outcome<unknown>, done: string): string | null => {
    if (!r.ok) return passkeyMessage(r);
    setView({ name: "idle" });
    notify(done);
    onChanged?.();
    void load();
    return null;
  };
  const common = {
    token,
    hasTotp: list.totp,
    hasPasskeys: list.passkeys.length > 0,
    onCancel: () => setView({ name: "idle" }),
  };
  const usable = list.available && supported;

  if (view.name === "codes")
    return (
      <section className="tf" aria-labelledby={`${id}-h`}>
        <h3 id={`${id}-h`}>{t("Save your recovery codes")}</h3>
        <Notice tone="warning" title={t("They are shown only now")}>
          {t(
            "Each code works once and lets you sign in if you lose your passkey. Keep them somewhere safe, apart from your password.",
          )}
        </Notice>
        <ul className="tf-codes" aria-label={t("Recovery codes")}>
          {view.codes.map((c) => (
            <li key={c}>
              <code>{c}</code>
            </li>
          ))}
        </ul>
        <Checkbox
          label={t("I have saved these codes")}
          checked={saved}
          onChange={(e) => setSaved(e.target.checked)}
        />
        <div className="tf-actions">
          <Button
            variant="primary"
            disabled={!saved}
            onClick={() => {
              setView({ name: "idle" });
              onAdded?.();
            }}
          >
            {t("Done")}
          </Button>
        </div>
      </section>
    );

  return (
    <section className="tf pk" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`}>
        {t("Passkeys")}{" "}
        <Badge tone={list.passkeys.length ? "success" : "neutral"}>
          {list.passkeys.length ? String(list.passkeys.length) : t("None")}
        </Badge>
      </h3>
      <p>
        {t(
          "Sign in with your fingerprint, face, screen lock or a security key. A passkey is a second step that cannot be phished, and you can let it replace the password.",
        )}
      </p>
      {!supported && (
        <Notice tone="info" title={t("This browser cannot use passkeys")}>
          {t("Try a current version of Chrome, Safari, Firefox or Edge.")}
        </Notice>
      )}
      {supported && !list.available && (
        <Notice tone="warning" title={t("Passkeys are not available at this address")}>
          {list.unavailable_reason ?? ""}
        </Notice>
      )}

      {list.passkeys.length > 0 && (
        <ul className="pk-list" aria-label={t("Your passkeys")}>
          {list.passkeys.map((p) => (
            <PasskeyRow
              key={p.id}
              p={p}
              onRename={() => {
                setName(p.name);
                setView({ name: "rename", id: p.id });
              }}
              onRemove={() => setView({ name: "remove", id: p.id })}
            />
          ))}
        </ul>
      )}

      {view.name === "rename" && (
        <ReauthForm
          {...common}
          heading={t("Rename passkey")}
          submitLabel={t("Rename")}
          onSubmit={async (reauth) => {
            return finished(
              await renamePasskey(token, view.id, name, reauth),
              t("Passkey renamed"),
            );
          }}
        >
          <Input
            label={t("Name")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={64}
            required
            autoFocus
          />
        </ReauthForm>
      )}

      {view.name === "remove" && (
        <ReauthForm
          {...common}
          heading={t("Remove passkey")}
          hint={
            list.policy_requires_second_factor && list.passkeys.length === 1 && !list.totp
              ? t(
                  "It is your only second step, and your administrator requires one: add another passkey or an authenticator app first.",
                )
              : t(
                  "You will no longer be able to sign in with it. Your other sessions are signed out.",
                )
          }
          submitLabel={t("Remove")}
          tone="danger"
          onSubmit={async (reauth) =>
            finished(await removePasskey(token, view.id, reauth), t("Passkey removed"))
          }
        />
      )}

      {view.name === "add" && (
        <ReauthForm
          {...common}
          heading={t("Add a passkey")}
          hint={t(
            "Confirm it is you. Then your browser asks you to create the passkey on this device or a security key.",
          )}
          submitLabel={t("Create passkey")}
          onSubmit={async (reauth) => {
            const r = await registerPasskey(token, reauth, name);
            if (!r.ok) return passkeyMessage(r);
            onChanged?.();
            void load();
            notify(t("Passkey added"));
            if (r.value.recovery_codes?.length) {
              setSaved(false);
              setView({ name: "codes", codes: r.value.recovery_codes, label: r.value.name });
            } else {
              setView({ name: "idle" });
              onAdded?.();
            }
            return null;
          }}
        >
          <Input
            label={t("Name")}
            hint={t("So you can tell them apart, for example “Work laptop” or “Phone”.")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={64}
            autoComplete="off"
            autoFocus
          />
        </ReauthForm>
      )}

      {view.name === "passwordless" && (
        <ReauthForm
          {...common}
          heading={
            view.on ? t("Allow signing in with a passkey alone") : t("Require a password again")
          }
          hint={
            view.on
              ? t(
                  "On the sign-in page you can then choose “Sign in with a passkey” and skip the password. Only passkeys that verify you (PIN, fingerprint or face) count.",
                )
              : undefined
          }
          submitLabel={view.on ? t("Allow") : t("Require password")}
          onSubmit={async (reauth) =>
            finished(
              await setPasswordless(token, view.on, reauth),
              view.on
                ? t("Signing in with a passkey alone is on")
                : t("Signing in with a passkey alone is off"),
            )
          }
        />
      )}

      {view.name === "idle" && (
        <>
          {list.passkeys.length > 0 && (
            <Switch
              label={t("Let me sign in with a passkey alone")}
              checked={list.passwordless}
              disabled={!list.passwordless && !list.passwordless_possible}
              onChange={(e) => setView({ name: "passwordless", on: e.target.checked })}
            />
          )}
          {list.passkeys.length > 0 && !list.passwordless && !list.passwordless_possible && (
            <p className="hint">
              {t(
                "None of your passkeys verified you when it was made, so they cannot replace the password. Add one that asks for your PIN, fingerprint or face.",
              )}
            </p>
          )}
          <div className="tf-actions">
            <Button
              variant="primary"
              icon="plus"
              disabled={!usable || list.passkeys.length >= list.max}
              onClick={() => {
                setName("");
                setView({ name: "add" });
              }}
            >
              {t("Add a passkey")}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}

function PasskeyRow({
  p,
  onRename,
  onRemove,
}: {
  p: Passkey;
  onRename: () => void;
  onRemove: () => void;
}) {
  return (
    <li className="pk-row">
      <Icon name="key" size={18} />
      <div className="pk-main">
        <strong className="pk-name">{p.name}</strong>
        <span className="hint">
          {t("Added {date}", { date: p.created_at ? fmt.dateTime(p.created_at) : "" })}
          {" · "}
          {p.last_used_at
            ? t("Last used {date}", { date: fmt.dateTime(p.last_used_at) })
            : t("Never used")}
        </span>
        <span className="pk-badges">
          {p.user_verified && (
            <Badge tone="success" dot={false}>
              {t("Verifies you")}
            </Badge>
          )}
          {p.backed_up && (
            <Badge tone="info" dot={false}>
              {t("Synced")}
            </Badge>
          )}
        </span>
      </div>
      <div className="pk-actions">
        <Button size="sm" variant="ghost" icon="edit" onClick={onRename}>
          {t("Rename")}
          <span className="sr-only"> {p.name}</span>
        </Button>
        <Button size="sm" variant="ghost" icon="trash" onClick={onRemove}>
          {t("Remove")}
          <span className="sr-only"> {p.name}</span>
        </Button>
      </div>
    </li>
  );
}

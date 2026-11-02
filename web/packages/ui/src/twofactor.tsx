import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { Icon } from "./icons.tsx";
import { QrCode } from "./qr.tsx";
import { Badge, Button, Checkbox, Input, Notice } from "./primitives.tsx";
import { t } from "./i18n/store.ts";

/* Two-factor authentication (D53), shared by the back office and the sign-up app: the second
   sign-in step and the setup panel. The apps give them an adapter for the HTTP routes. */

export type TwoFactorStatus = {
  /** The server has the key it needs to seal secrets. */
  available: boolean;
  enabled: boolean;
  pending: boolean;
  recovery_remaining: number;
  /** The session may only set this up (an administrator policy). */
  limited: boolean;
  is_admin?: boolean;
  require_admins?: boolean;
  require_all?: boolean;
  /** Passkeys (D65): how many, and whether any second factor is on (an app or a passkey). */
  passkeys?: number;
  second_factor?: boolean;
};
export type TwoFactorBegin = { secret: string; uri: string; issuer: string; account: string };
export type TwoFactorFail = { ok: false; status: number; code: string };
export type TwoFactorResult<T> = { ok: true; value: T } | TwoFactorFail;
/** One of the two ways to prove the second factor. */
export type Factor = { code?: string; recovery_code?: string };

export type TwoFactorApi = {
  status(): Promise<TwoFactorResult<TwoFactorStatus>>;
  begin(): Promise<TwoFactorResult<TwoFactorBegin>>;
  confirm(code: string): Promise<TwoFactorResult<{ recovery_codes: string[] }>>;
  disable(password: string, factor: Factor): Promise<TwoFactorResult<unknown>>;
  regenerate(
    password: string,
    factor: Factor,
  ): Promise<TwoFactorResult<{ recovery_codes: string[] }>>;
};

async function send<T>(
  method: "GET" | "POST",
  path: string,
  token: string,
  body?: object,
): Promise<TwoFactorResult<T>> {
  try {
    const res = await fetch(path, {
      method,
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const json = (await res.json().catch(() => ({}))) as { code?: string };
    if (res.ok) return { ok: true, value: json as T };
    return { ok: false, status: res.status, code: json.code ?? "" };
  } catch {
    return { ok: false, status: 0, code: "network" };
  }
}

/** The routes of D53 under `prefix` (`/api/auth` on an instance, `/api/platform` on the control plane). */
export function createTwoFactorApi(prefix: string, token: string): TwoFactorApi {
  return {
    status: () => send("GET", `${prefix}/mfa`, token),
    begin: () => send("POST", `${prefix}/mfa/begin`, token, {}),
    confirm: (code) => send("POST", `${prefix}/mfa/confirm`, token, { code }),
    disable: (password, factor) =>
      send("POST", `${prefix}/mfa/disable`, token, { password, ...factor }),
    regenerate: (password, factor) =>
      send("POST", `${prefix}/mfa/recovery`, token, { password, ...factor }),
  };
}

/** The wording for a failed call, by the server's stable error code. */
export function twoFactorMessage(f: { status: number; code: string }): string {
  switch (f.code) {
    case "invalid_code":
      return t("That code is not valid. Check the time on your device and try again.");
    case "invalid_password":
      return t("That password is not right.");
    case "mfa_expired":
      return t("This sign-in has expired. Start again.");
    case "rate_limited":
      return t("Too many attempts. Wait a minute and try again.");
    case "mfa_unavailable":
      return t(
        "Two-factor authentication is not set up on this server. Ask the operator to set the server key.",
      );
    case "network":
      return t("Could not reach the server.");
    case "invalid_input":
      return t("Check what you entered and try again.");
    default:
      return t("Something went wrong. Try again.");
  }
}

/** `ABCDEFGH…` as `ABCD EFGH …`, easy to read aloud and to type. */
export function groupKey(secret: string): string {
  return secret.replace(/(.{4})(?=.)/g, "$1 ");
}

/* ============ The second sign-in step ============ */

/**
 * "Enter the 6-digit code", with a way to use a recovery code instead, and a passkey when the
 * person has one (D65). `methods` says what can finish this sign-in (`totp`, `passkey`).
 * `onSubmit` and `onPasskey` resolve to an error message to show, or null when it went through
 * (the caller moves on).
 */
export function TwoFactorStep({
  onSubmit,
  onBack,
  methods = ["totp"],
  onPasskey,
}: {
  onSubmit: (factor: Factor) => Promise<string | null>;
  onBack: () => void;
  methods?: string[];
  onPasskey?: () => Promise<string | null>;
}) {
  const totp = methods.includes("totp");
  const passkey = methods.includes("passkey") && !!onPasskey;
  const [recovery, setRecovery] = useState(false);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showCode = totp || recovery;
  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const value = String(new FormData(e.currentTarget).get("code") ?? "").trim();
    if (!value) return;
    setBusy(true);
    setError(null);
    const err = await onSubmit(recovery ? { recovery_code: value } : { code: value });
    setBusy(false);
    if (err) setError(err);
  }
  async function usePasskey() {
    if (!onPasskey) return;
    setAsking(true);
    setError(null);
    const err = await onPasskey();
    setAsking(false);
    if (err) setError(err);
  }
  return (
    <form className="tf-step" onSubmit={(e) => void submit(e)}>
      <div className="signin-head">
        <h2>{t("Two-factor authentication")}</h2>
        <p className="hint">
          {recovery
            ? t("Enter one of your recovery codes. Each one works only once.")
            : totp
              ? passkey
                ? t("Enter the 6-digit code from your authenticator app, or use a passkey.")
                : t("Enter the 6-digit code from your authenticator app.")
              : t("Use a passkey you added to this account to finish signing in.")}
        </p>
      </div>
      {passkey && !recovery && !totp && (
        <Button
          variant="primary"
          size="lg"
          icon="key"
          loading={asking}
          autoFocus
          onClick={() => void usePasskey()}
        >
          {t("Use a passkey")}
        </Button>
      )}
      {showCode &&
        (recovery ? (
          <Input
            key="recovery"
            label={t("Recovery code")}
            name="code"
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            required
            autoFocus
            error={error ?? undefined}
          />
        ) : (
          <Input
            key="code"
            label={t("6-digit code")}
            name="code"
            inputMode="numeric"
            autoComplete="one-time-code"
            pattern="[0-9 ]*"
            maxLength={7}
            enterKeyHint="done"
            required
            autoFocus
            error={error ?? undefined}
          />
        ))}
      {!showCode && error && <Notice tone="danger" title={error} />}
      {showCode && (
        <Button type="submit" variant="primary" size="lg" loading={busy}>
          {t("Verify")}
        </Button>
      )}
      {passkey && totp && !recovery && (
        <Button icon="key" loading={asking} onClick={() => void usePasskey()}>
          {t("Use a passkey")}
        </Button>
      )}
      <div className="tf-links">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            setRecovery(!recovery);
            setError(null);
          }}
        >
          {recovery
            ? totp
              ? t("Use an authenticator code")
              : t("Use a passkey")
            : t("Use a recovery code")}
        </Button>
        <Button variant="ghost" size="sm" onClick={onBack}>
          {t("Back to sign in")}
        </Button>
      </div>
    </form>
  );
}

/* ============ The setup panel ============ */

type View =
  | { name: "loading" }
  | { name: "failed"; message: string }
  | { name: "status"; s: TwoFactorStatus }
  | { name: "enrol"; s: TwoFactorStatus; info: TwoFactorBegin }
  | { name: "codes"; s: TwoFactorStatus; codes: string[]; fresh: boolean; account: string };

function recoveryText(account: string, codes: string[]): string {
  return [t("Recovery codes for {account}", { account }), "", ...codes, ""].join("\n");
}

/**
 * Turn two-factor on or off, see how many recovery codes are left and make new ones. `onActive`
 * runs after a person has confirmed they saved their recovery codes, when two-factor is on.
 */
export function TwoFactorPanel({
  api,
  onActive,
  onChanged,
  notify,
}: {
  api: TwoFactorApi;
  onActive?: () => void;
  /** Called whenever the status may have changed, so a page can refresh what depends on it. */
  onChanged?: () => void;
  notify?: (text: string, tone?: "ok" | "error" | "info") => void;
}) {
  const [view, setView] = useState<View>({ name: "loading" });
  const [action, setAction] = useState<null | "disable" | "regenerate">(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recovery, setRecovery] = useState(false);
  const [saved, setSaved] = useState(false);
  const account = useRef("");
  const heading = useId();

  async function load() {
    const r = await api.status();
    if (!r.ok) return setView({ name: "failed", message: twoFactorMessage(r) });
    account.current = (r.value as TwoFactorStatus & { user?: string }).user ?? account.current;
    setView({ name: "status", s: r.value });
  }
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  const status = view.name === "loading" || view.name === "failed" ? null : view.s;

  async function begin() {
    if (!status) return;
    setBusy(true);
    setError(null);
    const r = await api.begin();
    setBusy(false);
    if (!r.ok) return setError(twoFactorMessage(r));
    account.current = r.value.account;
    setView({ name: "enrol", s: status, info: r.value });
  }

  async function confirm(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (view.name !== "enrol") return;
    const code = String(new FormData(e.currentTarget).get("code") ?? "").trim();
    setBusy(true);
    setError(null);
    const r = await api.confirm(code);
    setBusy(false);
    if (!r.ok) return setError(twoFactorMessage(r));
    setSaved(false);
    setView({
      name: "codes",
      s: {
        ...view.s,
        enabled: true,
        pending: false,
        recovery_remaining: r.value.recovery_codes.length,
      },
      codes: r.value.recovery_codes,
      fresh: true,
      account: view.info.account,
    });
    onChanged?.();
  }

  async function reauth(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!status || !action) return;
    const f = new FormData(e.currentTarget);
    const password = String(f.get("password") ?? "");
    const value = String(f.get("code") ?? "").trim();
    const factor: Factor = recovery ? { recovery_code: value } : { code: value };
    setBusy(true);
    setError(null);
    if (action === "disable") {
      const r = await api.disable(password, factor);
      setBusy(false);
      if (!r.ok) return setError(twoFactorMessage(r));
      setAction(null);
      notify?.(t("Two-factor authentication is off"));
      onChanged?.();
      await load();
    } else {
      const r = await api.regenerate(password, factor);
      setBusy(false);
      if (!r.ok) return setError(twoFactorMessage(r));
      setAction(null);
      setSaved(false);
      setView({
        name: "codes",
        s: { ...status, recovery_remaining: r.value.recovery_codes.length },
        codes: r.value.recovery_codes,
        fresh: false,
        account: account.current,
      });
      onChanged?.();
    }
  }

  if (view.name === "loading")
    return (
      <p className="hint" role="status">
        {t("Loading")}
      </p>
    );
  if (view.name === "failed") return <Notice tone="danger" title={view.message} />;

  if (view.name === "codes") {
    const text = recoveryText(view.account, view.codes);
    return (
      <section className="tf" aria-labelledby={heading}>
        <h3 id={heading}>{t("Save your recovery codes")}</h3>
        <Notice tone="warning" title={t("They are shown only now")}>
          {t(
            "Each code works once and lets you sign in if you lose your phone. Keep them somewhere safe, apart from your password.",
          )}
        </Notice>
        <ul className="tf-codes" aria-label={t("Recovery codes")}>
          {view.codes.map((c) => (
            <li key={c}>
              <code>{c}</code>
            </li>
          ))}
        </ul>
        <div className="tf-actions">
          <Button
            icon="copy"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(text);
                notify?.(t("Recovery codes copied"));
              } catch {
                notify?.(t("Could not copy. Select the codes and copy them by hand."), "error");
              }
            }}
          >
            {t("Copy")}
          </Button>
          <Button
            icon="download"
            onClick={() => {
              const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
              const a = document.createElement("a");
              a.href = url;
              a.download = "obop-recovery-codes.txt";
              a.click();
              setTimeout(() => URL.revokeObjectURL(url), 1000);
            }}
          >
            {t("Download")}
          </Button>
        </div>
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
              setView({ name: "status", s: view.s });
              if (view.fresh) onActive?.();
            }}
          >
            {t("Done")}
          </Button>
        </div>
      </section>
    );
  }

  if (view.name === "enrol") {
    return (
      <section className="tf" aria-labelledby={heading}>
        <h3 id={heading}>{t("Set up two-factor authentication")}</h3>
        <ol className="tf-steps">
          <li>
            <p>
              {t(
                "Scan this code with an authenticator app on your phone, or type the key into the app.",
              )}
            </p>
            <div className="tf-key-row">
              <QrCode value={view.info.uri} label={t("QR code for the authenticator app")} />
              <div className="tf-key">
                <span className="overline">{t("Key")}</span>
                <code className="tf-secret" aria-label={t("Setup key")}>
                  {groupKey(view.info.secret)}
                </code>
                <p className="hint">
                  {t("Time-based, 6 digits, account {account}.", { account: view.info.account })}
                </p>
              </div>
            </div>
          </li>
          <li>
            <form className="tf-form" onSubmit={(e) => void confirm(e)}>
              <Input
                label={t("6-digit code")}
                hint={t("Enter the code the app shows now to turn two-factor on.")}
                name="code"
                inputMode="numeric"
                autoComplete="one-time-code"
                pattern="[0-9 ]*"
                maxLength={7}
                required
                autoFocus
                error={error ?? undefined}
              />
              <div className="tf-actions">
                <Button type="submit" variant="primary" loading={busy}>
                  {t("Turn on")}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setError(null);
                    setView({ name: "status", s: view.s });
                  }}
                >
                  {t("Cancel")}
                </Button>
              </div>
            </form>
          </li>
        </ol>
      </section>
    );
  }

  const s = view.s;
  if (!s.enabled)
    return (
      <section className="tf" aria-labelledby={heading}>
        <h3 id={heading}>
          {t("Two-factor authentication")} <Badge tone="neutral">{t("Off")}</Badge>
        </h3>
        {!s.available ? (
          <Notice tone="warning" title={t("Not available on this server")}>
            {t(
              "Two-factor authentication needs a server key to protect the secrets. Ask the operator to set it up.",
            )}
          </Notice>
        ) : (
          <>
            <p>
              {t(
                "Add a second step to signing in: a 6-digit code from an app on your phone. Even if your password leaks, nobody can sign in without it.",
              )}
            </p>
            {error && <Notice tone="danger" title={error} />}
            <div className="tf-actions">
              <Button
                variant="primary"
                icon="shield-check"
                loading={busy}
                onClick={() => void begin()}
              >
                {t("Set up two-factor")}
              </Button>
            </div>
          </>
        )}
      </section>
    );

  return (
    <section className="tf" aria-labelledby={heading}>
      <h3 id={heading}>
        {t("Two-factor authentication")} <Badge tone="success">{t("On")}</Badge>
      </h3>
      <p>
        {t("{count, plural, one {# recovery code left.} other {# recovery codes left.}}", {
          count: s.recovery_remaining,
        })}
      </p>
      {s.recovery_remaining <= 2 && (
        <Notice tone="warning" title={t("You are running low on recovery codes")}>
          {t("Create new ones now, while you can still sign in with your app.")}
        </Notice>
      )}
      {action ? (
        <form className="tf-form" onSubmit={(e) => void reauth(e)}>
          <h4>
            {action === "disable" ? t("Turn off two-factor") : t("Create new recovery codes")}
          </h4>
          <p className="hint">{t("Confirm it is you: your password and a current code.")}</p>
          <Input
            label={t("Password")}
            name="password"
            type="password"
            autoComplete="current-password"
            required
            autoFocus
          />
          <Input
            key={recovery ? "recovery" : "code"}
            label={recovery ? t("Recovery code") : t("6-digit code")}
            name="code"
            inputMode={recovery ? undefined : "numeric"}
            autoComplete={recovery ? "off" : "one-time-code"}
            pattern={recovery ? undefined : "[0-9 ]*"}
            maxLength={recovery ? undefined : 7}
            spellCheck={false}
            required
            error={error ?? undefined}
          />
          <div className="tf-actions">
            <Button
              type="submit"
              variant={action === "disable" ? "danger" : "primary"}
              loading={busy}
            >
              {action === "disable" ? t("Turn off") : t("Create new codes")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setAction(null);
                setError(null);
              }}
            >
              {t("Cancel")}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setRecovery(!recovery)}>
              {recovery ? t("Use an authenticator code") : t("Use a recovery code")}
            </Button>
          </div>
        </form>
      ) : (
        <div className="tf-actions">
          <Button
            icon="key"
            onClick={() => {
              setAction("regenerate");
              setRecovery(false);
              setError(null);
            }}
          >
            {t("New recovery codes")}
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              setAction("disable");
              setRecovery(false);
              setError(null);
            }}
          >
            {t("Turn off two-factor")}
          </Button>
        </div>
      )}
      <p className="hint tf-note">
        <Icon name="info" size={14} />{" "}
        {t(
          "Lost your phone and your codes? Ask an administrator or the operator to reset your two-factor.",
        )}
      </p>
    </section>
  );
}

import { useState } from "react";
import { LanguageSelect, TwoFactorStep, t, twoFactorMessage, useI18n, type Factor } from "@obop/ui";
import { login, loginMfa } from "./api.ts";
import { passkeyMessage } from "./Passkeys.tsx";
import { passkeysSupported, signInWithPasskey } from "./passkeys.ts";
import { storageFaultMessage } from "./serverMessage.ts";
import { Button, Icon, Input, Logo, Notice } from "./ui.tsx";

/**
 * Sign in. A person with two-factor authentication on sees a second step after the password
 * (D53): the code from their app, a recovery code or a passkey (D65). A person who allowed it can
 * also sign in with a passkey alone.
 */
export function SignIn({
  onDone,
}: {
  /** `enrolment`: the session may only set up two-factor authentication. */
  onDone: (token: string, user: string, enrolment: boolean) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<{ mfaToken: string; user: string; methods: string[] } | null>(
    null,
  );
  const [asking, setAsking] = useState(false);
  const i18n = useI18n();

  async function password(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    const user = String(f.get("username"));
    setBusy(true);
    setError(null);
    const r = await login(user, String(f.get("password")));
    setBusy(false);
    if (r.kind === "session") onDone(r.token, user, r.enrolment);
    else if (r.kind === "mfa") setStep({ mfaToken: r.mfaToken, user, methods: r.methods });
    else if (r.status === 429) setError(twoFactorMessage({ status: 429, code: "rate_limited" }));
    else if (r.status === 0) setError(twoFactorMessage({ status: 0, code: "network" }));
    else setError(storageFaultMessage(r.status, r.code) ?? t("Invalid username or password"));
  }

  /** Sign in with a passkey alone (the person allowed it in Security). */
  async function passwordless() {
    setAsking(true);
    setError(null);
    const r = await signInWithPasskey();
    setAsking(false);
    if (r.ok) onDone(r.value.token, r.value.user ?? "", !!r.value.mfa_enrolment_required);
    else if (r.kind === "cancelled") setError(passkeyMessage(r));
    else if (r.kind === "server" && r.status === 429) setError(passkeyMessage(r));
    else if (r.kind === "server" && storageFaultMessage(r.status, r.code))
      setError(storageFaultMessage(r.status, r.code));
    else
      setError(
        r.kind === "server"
          ? t(
              "That passkey could not be used to sign in. It may not be set up for signing in without a password. Use your password instead.",
            )
          : passkeyMessage(r),
      );
  }

  /** The second step with a passkey. */
  async function passkeyStep(): Promise<string | null> {
    if (!step) return null;
    const r = await signInWithPasskey(step.mfaToken);
    if (r.ok) {
      onDone(r.value.token, step.user, !!r.value.mfa_enrolment_required);
      return null;
    }
    if (r.kind === "server" && r.code === "mfa_expired") {
      setStep(null);
      setError(twoFactorMessage(r));
      return null;
    }
    if (r.kind === "server") {
      const busy = storageFaultMessage(r.status, r.code);
      if (busy) return busy;
    }
    return passkeyMessage(r);
  }

  async function code(factor: Factor): Promise<string | null> {
    if (!step) return null;
    const r = await loginMfa(step.mfaToken, factor);
    if (r.kind === "session") {
      onDone(r.token, step.user, r.enrolment);
      return null;
    }
    if (r.kind === "error" && r.code === "mfa_expired") {
      setStep(null);
      setError(twoFactorMessage(r));
      return null;
    }
    if (r.kind === "error") {
      const busy = storageFaultMessage(r.status, r.code);
      if (busy) return busy;
    }
    return twoFactorMessage(r.kind === "error" ? r : { status: 0, code: "" });
  }

  return (
    <main className="signin-wrap">
      <aside className="signin-brand">
        <h1 className="signin-logo-lg">
          <Logo variant="full" size={28} title="obop" />
        </h1>
        <p className="signin-claim">{t("One business, one platform.")}</p>
        <ul>
          <li>
            <Icon name="users" size={18} /> {t("Every app shares one set of contacts and products")}
          </li>
          <li>
            <Icon name="shield-check" size={18} />{" "}
            {t("Personal data is masked unless you may see it")}
          </li>
          <li>
            <Icon name="sparkle" size={18} />{" "}
            {t("Agents can do what you can, and ask before they act")}
          </li>
        </ul>
      </aside>
      {step ? (
        <div className="signin">
          <h1 className="signin-logo">
            <Logo variant="full" size={30} title="obop" />
          </h1>
          <TwoFactorStep
            key={step.mfaToken}
            methods={step.methods}
            onPasskey={passkeysSupported() ? passkeyStep : undefined}
            onSubmit={code}
            onBack={() => {
              setStep(null);
              setError(null);
            }}
          />
          <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
        </div>
      ) : (
        <form className="signin" onSubmit={(e) => void password(e)}>
          <h1 className="signin-logo">
            <Logo variant="full" size={30} title="obop" />
          </h1>
          <div className="signin-head">
            <h2>{t("Sign in")}</h2>
            <p className="hint">{t("Sign in to your instance.")}</p>
          </div>
          <Input label={t("Username")} name="username" autoComplete="username" required autoFocus />
          <Input
            label={t("Password")}
            name="password"
            type="password"
            autoComplete="current-password"
            required
          />
          {error && <Notice tone="danger" title={error} />}
          <Button type="submit" variant="primary" size="lg" loading={busy}>
            {t("Sign in")}
          </Button>
          {passkeysSupported() && (
            <Button icon="key" size="lg" loading={asking} onClick={() => void passwordless()}>
              {t("Sign in with a passkey")}
            </Button>
          )}
          <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
        </form>
      )}
    </main>
  );
}

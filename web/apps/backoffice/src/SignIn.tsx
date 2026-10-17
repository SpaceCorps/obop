import { useState } from "react";
import { LanguageSelect, TwoFactorStep, t, twoFactorMessage, useI18n, type Factor } from "@obop/ui";
import { login, loginMfa } from "./api.ts";
import { Button, Icon, Input, Logo, Notice } from "./ui.tsx";

/**
 * Sign in. A person with two-factor authentication on sees a second step after the password
 * (D53): the code from their app, or a recovery code.
 */
export function SignIn({
  onDone,
}: {
  /** `enrolment`: the session may only set up two-factor authentication. */
  onDone: (token: string, user: string, enrolment: boolean) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<{ mfaToken: string; user: string } | null>(null);
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
    else if (r.kind === "mfa") setStep({ mfaToken: r.mfaToken, user });
    else if (r.status === 429) setError(twoFactorMessage({ status: 429, code: "rate_limited" }));
    else if (r.status === 0) setError(twoFactorMessage({ status: 0, code: "network" }));
    else setError(t("Invalid username or password"));
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
          <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
        </form>
      )}
    </main>
  );
}

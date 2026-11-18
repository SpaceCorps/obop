import { useEffect, useRef, useState } from "react";
import { InstanceLogo, LanguageSelect, t, twoFactorMessage, useI18n, type Wording } from "@obop/ui";
import { setupCreate, type LoginResult } from "./api.ts";
import { storageFaultMessage } from "./serverMessage.ts";
import { Button, Input, Notice } from "./ui.tsx";

/** At least this many characters, as the server asks (counted as the server counts them). */
const MIN_PASSWORD = 12;
const USERNAME = /^[A-Za-z0-9._\-@]+$/;

type Field = "instance_name" | "username" | "password" | "repeat";
/** The inputs of the form: what is sent to the server. */
const FIELDS = ["instance_name", "code", "username", "password", "repeat"];

/**
 * What the person is told for a refusal of the server. With `field`, the sentence belongs to that
 * field: it is shown under it and the field takes the focus. Kept as how to word it, not as a
 * finished sentence, so a switch of the language on this page words it again (D9).
 */
type Failure = { word: Wording; closed?: boolean; needsKey?: boolean; field?: Field };

export function setupFailure(r: Extract<LoginResult, { kind: "error" }>): Failure {
  if (r.status === 403 && r.code === "setup_code_invalid")
    return {
      word: () =>
        t(
          "The set-up code is not right, or it has run out. Check the log of the server, or restart it to print a new code.",
        ),
    };
  if (r.status === 429)
    return { word: () => t("Too many attempts. Wait a few minutes and try again.") };
  if (r.status === 409 && r.code === "setup_closed")
    return {
      word: () => t("This instance already has an administrator. Sign in with that account."),
      closed: true,
    };
  if (r.code === "setup_needs_secret_key") return { word: needsKeyText, needsKey: true };
  if (storageFaultMessage(r.status, r.code))
    return { word: () => storageFaultMessage(r.status, r.code) ?? "" };
  // The instance name is the one input the form cannot check itself (the brand's rules): the
  // server gives it a code of its own. The server's own sentence is English and names a rule, so
  // it is never shown.
  if (r.status === 400 && r.code === "setup_instance_name_invalid")
    return {
      word: () =>
        t(
          "This name of the instance cannot be used. Choose a plain name, or leave the field empty.",
        ),
      field: "instance_name",
    };
  if (r.status === 400) return { word: () => t("Check what you entered and try again.") };
  return {
    word: () => twoFactorMessage({ status: r.status, code: r.status === 0 ? "network" : r.code }),
  };
}

function needsKeyText(): string {
  return t(
    "This instance has no server secret yet, so two-factor sign-in cannot be set up. Set OBOP_SECRET_KEY in the environment of this service, restart it, and come back to this page.",
  );
}

/**
 * The first administrator of a new instance (D96): shown by the sign-in page instead of its form
 * while no account exists. The set-up code from the server log proves that the person runs the
 * server; the answer is the one a password sign-in gives, so the forced two-factor enrolment
 * follows at once. The name of the instance comes first: an authenticator app keeps the name it
 * was given at enrolment.
 */
export function SetUp({
  codeActive,
  needsSecretKey,
  onDone,
  onClosed,
}: {
  codeActive: boolean;
  needsSecretKey: boolean;
  onDone: (token: string, user: string, enrolment: boolean) => void;
  /** An account exists after all: show the normal sign-in form. */
  onClosed: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  // Field messages are kept as how to word them too (D9).
  const [errors, setErrors] = useState<Partial<Record<Field, Wording>>>({});
  const [needsKey, setNeedsKey] = useState(needsSecretKey);
  const alert = useRef<HTMLDivElement>(null);
  const i18n = useI18n();
  // Focus goes to the notice of a refusal, so a screen reader reads it and the keyboard is there.
  useEffect(() => {
    if (failure) alert.current?.focus();
  }, [failure]);

  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const f = new FormData(form);
    const value = (k: string) => String(f.get(k) ?? "");
    const username = value("username").trim();
    const password = value("password");
    const found: Partial<Record<Field, Wording>> = {};
    if (!USERNAME.test(username))
      found.username = () => t("The username may only use letters, digits and . _ - @");
    if ([...password].length < MIN_PASSWORD)
      found.password = () => t("The password must be at least 12 characters.");
    if (password !== value("repeat")) found.repeat = () => t("The two passwords are not the same.");
    setErrors(found);
    setFailure(null);
    const first = (["username", "password", "repeat"] as const).find((k) => found[k]);
    if (first) {
      form.querySelector<HTMLInputElement>(`[name="${first}"]`)?.focus();
      return;
    }
    setBusy(true);
    const name = value("instance_name").trim();
    const r = await setupCreate({
      code: value("code"),
      username,
      password,
      ...(name ? { instance_name: name } : {}),
    });
    setBusy(false);
    if (r.kind === "session") onDone(r.token, username.toLowerCase(), r.enrolment);
    else if (r.kind === "error") {
      const why = setupFailure(r);
      if (why.needsKey) setNeedsKey(true);
      if (why.field) {
        setErrors({ [why.field]: why.word });
        form.querySelector<HTMLInputElement>(`[name="${why.field}"]`)?.focus();
      } else setFailure(why);
    } else setFailure({ word: () => twoFactorMessage({ status: 0, code: "" }) });
  }

  // A refusal of the server is about what was sent: once a field changes it no longer describes
  // the form, so it goes. "Already has an administrator" stays: no field changes that. The
  // language below is no field of the form: a switch words the refusal again (D9).
  function changed(e: React.FormEvent<HTMLFormElement>) {
    const name = (e.target as HTMLInputElement).name;
    if (failure && !failure.closed && FIELDS.includes(name)) setFailure(null);
  }

  const off = needsKey;
  return (
    <form className="signin setup" onSubmit={(e) => void submit(e)} onChange={changed} noValidate>
      <h1 className="signin-logo">
        <InstanceLogo variant="full" size={30} />
      </h1>
      <div className="signin-head">
        <h2>{t("Set up this instance")}</h2>
        <p className="hint">
          {t(
            "Create the first administrator. This page works only until the first account exists.",
          )}
        </p>
      </div>
      {needsKey ? (
        <Notice tone="warning" title={needsKeyText()} />
      ) : (
        !codeActive && (
          <Notice
            tone="warning"
            title={t(
              "No set-up code is active. Restart the server: it prints a new code when it starts.",
            )}
          />
        )
      )}
      <Input
        label={t("Name of this instance (optional)")}
        hint={t(
          "Shown on the sign-in page and in your authenticator app. An authenticator app keeps the name it was given, so choose it now.",
        )}
        name="instance_name"
        autoComplete="organization"
        maxLength={120}
        disabled={off}
        autoFocus={!off}
        error={errors.instance_name?.()}
      />
      <Input
        label={t("Set-up code")}
        hint={t(
          "Printed in the server log when the server started. On a hosting service, open the log of this service and find the line that starts with “Set-up code”. It works for 60 minutes.",
        )}
        name="code"
        type="text"
        autoComplete="off"
        autoCapitalize="characters"
        spellCheck={false}
        required
        disabled={off}
      />
      <Input
        label={t("Username")}
        hint={t("Letters, digits and . _ - @")}
        name="username"
        autoComplete="username"
        autoCapitalize="none"
        spellCheck={false}
        required
        disabled={off}
        error={errors.username?.()}
      />
      <Input
        label={t("Password")}
        hint={t("At least 12 characters.")}
        name="password"
        type="password"
        autoComplete="new-password"
        required
        disabled={off}
        error={errors.password?.()}
      />
      <Input
        label={t("Repeat the password")}
        name="repeat"
        type="password"
        autoComplete="new-password"
        required
        disabled={off}
        error={errors.repeat?.()}
      />
      {failure && (
        <div ref={alert} tabIndex={-1} className="setup-alert">
          <Notice
            tone="danger"
            title={failure.word()}
            action={
              failure.closed ? <Button onClick={onClosed}>{t("Go to sign in")}</Button> : undefined
            }
          />
        </div>
      )}
      <Button type="submit" variant="primary" size="lg" loading={busy} disabled={off}>
        {t("Create the administrator")}
      </Button>
      <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
    </form>
  );
}

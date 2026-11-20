import { useEffect, useRef, useState } from "react";
import {
  InstanceLogo,
  LanguageSelect,
  TwoFactorStep,
  brandMessageIn,
  pageBrand,
  t,
  twoFactorMessage,
  useI18n,
  useWorded,
  type Factor,
  type Wording,
} from "@obop/ui";
import {
  SSO_START,
  forgetPlace,
  login,
  loginMfa,
  rememberPlace,
  signedInLogin,
  ssoEnabled,
  ssoExchange,
  takeSsoReturn,
  type SsoErrorWord,
} from "./api.ts";
import { SignInNotes } from "./host/SignInNotes.tsx";
import { useHost } from "./host/useHost.ts";
import { passkeyMessage } from "./Passkeys.tsx";
import {
  autofillAvailable,
  passkeyAutofill,
  passkeysSupported,
  signInWithPasskey,
  type Outcome,
  type SignedIn,
} from "./passkeys.ts";
import { storageFaultMessage } from "./serverMessage.ts";
import { Button, Icon, Input, Notice } from "./ui.tsx";

/**
 * What the person is told when the sign-in with Microsoft did not go through (D91). Four words
 * come back from the server and nothing else: the reason is in the security log, not here.
 */
function ssoMessage(word: SsoErrorWord): string {
  switch (word) {
    case "cancelled":
      return t("The sign-in with Microsoft was cancelled.");
    case "unavailable":
      return t("Sign-in with Microsoft is not available right now. Try again in a few minutes.");
    case "busy":
      return t("Too many attempts. Wait a minute and try again.");
    default:
      return t(
        "The sign-in with Microsoft could not be completed. If it keeps happening, ask your administrator.",
      );
  }
}

/**
 * Sign in. A person with two-factor authentication on sees a second step after the password
 * (D53): the code from their app, a recovery code or a passkey (D65). A person who allowed it can
 * also sign in with a passkey alone, and where an administrator set it up, with Microsoft (D91):
 * the button leaves for the server, which sends the browser to Microsoft and back with a one-time
 * code that is exchanged here for the same answer the password gives.
 */
export function SignIn({
  onDone,
}: {
  /** `enrolment`: the session may only set up two-factor authentication. */
  onDone: (token: string, user: string, enrolment: boolean) => void;
}) {
  // Kept as how to word it, so a switch of the language below words it again (D9).
  const [error, setError] = useWorded();
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<{ mfaToken: string; user: string; methods: string[] } | null>(
    null,
  );
  const [asking, setAsking] = useState(false);
  // "Forgot your password?" is open: obop sends no mail yet, so the way back is an administrator.
  const [forgot, setForgot] = useState(false);
  const [microsoft, setMicrosoft] = useState(false);
  // The browser is on its way to Microsoft: the button waits, so that a second click does not
  // begin a second sign-in.
  const [leaving, setLeaving] = useState(false);
  const i18n = useI18n();
  const host = useHost();
  // The brand of the instance, from the page (docs/spec/branding.md): with one, the logo and the
  // name stand in the aside instead of the product's own copy, and its message sits under the hint.
  const brand = pageBrand();
  const message = brandMessageIn(brand, i18n.lang);
  const form = useRef<HTMLFormElement>(null);
  // The passkeys offered in the autofill of the username field (E7c): the offer that waits now,
  // ended before a button asks the browser (it takes one request at a time); `rearm` starts a new
  // one after a button's request is done.
  const offer = useRef<AbortController | null>(null);
  const [rearm, setRearm] = useState(0);

  // The Back button of the browser restores this page as it was left, from its back/forward cache
  // (`persisted`), with the button still waiting: it must be usable again.
  useEffect(() => {
    const restored = (e: PageTransitionEvent) => {
      if (e.persisted) setLeaving(false);
    };
    window.addEventListener("pageshow", restored);
    return () => window.removeEventListener("pageshow", restored);
  }, []);

  // The button is shown only where an administrator turned the sign-in on.
  useEffect(() => {
    let live = true;
    void ssoEnabled().then((on) => live && setMicrosoft(on));
    return () => {
      live = false;
    };
  }, []);

  // Back from Microsoft: the address carried a one-time code or one word about what went wrong.
  useEffect(() => {
    const back = takeSsoReturn();
    if (!back) return;
    if ("error" in back) {
      forgetPlace();
      setError(() => ssoMessage(back.error));
      return;
    }
    setBusy(true);
    void ssoExchange(back.code).then((r) => {
      setBusy(false);
      if (r.kind === "session") onDone(r.token, r.user ?? "", r.enrolment);
      else if (r.kind === "mfa")
        setStep({ mfaToken: r.mfaToken, user: r.user ?? "", methods: r.methods });
      else {
        // The round trip failed: the place kept for it is not for a later password sign-in.
        forgetPlace();
        if (r.status === 429) setError(() => ssoMessage("busy"));
        else if (r.status === 0 || r.status >= 500) setError(() => ssoMessage("unavailable"));
        else setError(() => ssoMessage("failed"));
      }
    });
  }, [onDone]);

  // Offer passkeys in the autofill while the password form is shown. The challenge of an offer lives
  // two minutes on the server, so a new offer replaces it every 100 seconds.
  useEffect(() => {
    if (step) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const arm = async () => {
      if (!live || !(await autofillAvailable()) || !live) return;
      const c = new AbortController();
      offer.current = c;
      timer = setTimeout(() => {
        c.abort();
        void arm();
      }, 100_000);
      const r = await passkeyAutofill(c.signal);
      clearTimeout(timer);
      if (offer.current === c) offer.current = null;
      if (!live || r === null) return;
      if (r.ok) {
        onDone(r.value.token, r.value.user ?? "", !!r.value.mfa_enrolment_required);
        return;
      }
      // The browser ended the offer itself (no authenticator, not supported): nobody picked a key,
      // so nothing is said; the passkey button still works.
      if (r.kind !== "server") return;
      // A key picked from the autofill that did not sign in: the offer ends with the message, so
      // that a refusal is not asked for again and again; the passkey button offers it once more.
      passkeyFailed(r);
    };
    void arm();
    return () => {
      live = false;
      clearTimeout(timer);
      offer.current?.abort();
      offer.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, rearm]);

  /** Signed in: the login kept is the one the server names for the session (U11), else `fallback`. */
  async function finish(token: string, fallback: string, enrolment: boolean) {
    onDone(token, (await signedInLogin(token)) ?? fallback, enrolment);
  }

  async function password(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    const typed = String(f.get("username"));
    setBusy(true);
    setError(null);
    const r = await login(typed, String(f.get("password")));
    // The login the server signed in (`$user` of a filter or a read-only hint compares with it,
    // U11): its answer's when it names one, else what was typed as the server reads a login,
    // trimmed and in lower case (" Sam" is "sam"), until the session says (finish).
    const user = (r.kind !== "error" && r.user) || typed.trim().toLowerCase();
    if (r.kind === "session") {
      await finish(r.token, user, r.enrolment);
    }
    setBusy(false);
    if (r.kind === "session") return;
    if (r.kind === "mfa") setStep({ mfaToken: r.mfaToken, user, methods: r.methods });
    else if (r.status === 429)
      setError(() => twoFactorMessage({ status: 429, code: "rate_limited" }));
    else if (r.status === 0) setError(() => twoFactorMessage({ status: 0, code: "network" }));
    else setError(() => storageFaultMessage(r.status, r.code) ?? t("Invalid username or password"));
  }

  /**
   * Sign in with a passkey alone (the person allowed it in Security). With a name typed, the keys
   * of that name are asked for (a key that does not keep the name works too).
   */
  async function passwordless() {
    setAsking(true);
    setError(null);
    offer.current?.abort();
    offer.current = null;
    const typed = form.current ? String(new FormData(form.current).get("username") ?? "") : "";
    const r = await signInWithPasskey(undefined, typed);
    setAsking(false);
    if (r.ok) onDone(r.value.token, r.value.user ?? "", !!r.value.mfa_enrolment_required);
    else {
      // With a name typed the browser offers only that name's keys: a typo or another person's
      // name looks the same to the browser as a cancel, so say both.
      if (r.kind === "cancelled" && typed.trim())
        setError(() =>
          t(
            "No passkey was used. This device may have no passkey for that username: check it, or clear the field to choose any passkey on this device.",
          ),
        );
      else passkeyFailed(r);
      setRearm((n) => n + 1);
    }
  }

  /** Why a passkey alone did not sign in. */
  function passkeyFailed(r: Exclude<Outcome<SignedIn>, { ok: true }>) {
    if (r.kind === "cancelled") setError(() => passkeyMessage(r));
    else if (r.kind === "server" && r.status === 429) setError(() => passkeyMessage(r));
    else if (r.kind === "server" && storageFaultMessage(r.status, r.code))
      setError(() => storageFaultMessage(r.status, r.code) ?? "");
    else
      setError(() =>
        r.kind === "server"
          ? t(
              "That passkey could not be used to sign in. It may not be set up for signing in without a password. Use your password instead.",
            )
          : passkeyMessage(r),
      );
  }

  /** The second step with a passkey. */
  async function passkeyStep(): Promise<Wording | null> {
    if (!step) return null;
    const r = await signInWithPasskey(step.mfaToken);
    if (r.ok) {
      await finish(r.value.token, step.user, !!r.value.mfa_enrolment_required);
      return null;
    }
    if (r.kind === "server" && r.code === "mfa_expired") {
      setStep(null);
      setError(() => twoFactorMessage(r));
      return null;
    }
    if (r.kind === "server") {
      if (storageFaultMessage(r.status, r.code))
        return () => storageFaultMessage(r.status, r.code)!;
    }
    return () => passkeyMessage(r);
  }

  async function code(factor: Factor): Promise<Wording | null> {
    if (!step) return null;
    const r = await loginMfa(step.mfaToken, factor);
    if (r.kind === "session") {
      await finish(r.token, step.user, r.enrolment);
      return null;
    }
    if (r.kind === "error" && r.code === "mfa_expired") {
      setStep(null);
      setError(() => twoFactorMessage(r));
      return null;
    }
    if (r.kind === "error") {
      if (storageFaultMessage(r.status, r.code))
        return () => storageFaultMessage(r.status, r.code)!;
    }
    return () => twoFactorMessage(r.kind === "error" ? r : { status: 0, code: "" });
  }

  return (
    <main className="signin-wrap">
      <aside className="signin-brand">
        <h1 className="signin-logo-lg">
          <InstanceLogo variant="full" size={brand ? 40 : 28} />
        </h1>
        {!brand && (
          <>
            <p className="signin-claim">{t("One business, one platform.")}</p>
            <ul>
              <li>
                <Icon name="users" size={18} />{" "}
                {t("Every app shares one set of contacts and products")}
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
          </>
        )}
      </aside>
      {step ? (
        <div className="signin">
          <h1 className="signin-logo">
            <InstanceLogo variant="full" size={30} />
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
        <form ref={form} className="signin" onSubmit={(e) => void password(e)}>
          <h1 className="signin-logo">
            <InstanceLogo variant="full" size={30} />
          </h1>
          <div className="signin-head">
            <h2>{t("Sign in")}</h2>
            <p className="hint">{t("Sign in to your instance.")}</p>
            {message.text && (
              <p
                className="hint signin-message"
                lang={message.lang === i18n.lang ? undefined : message.lang}
              >
                {message.text}
              </p>
            )}
          </div>
          <Input
            label={t("Username")}
            name="username"
            autoComplete="username webauthn"
            required
            autoFocus
          />
          <Input
            label={t("Password")}
            name="password"
            type="password"
            autoComplete="current-password"
            required
          />
          <div className="signin-forgot">
            <button
              type="button"
              className="linklike"
              aria-expanded={forgot}
              aria-controls="signin-forgot-help"
              onClick={() => setForgot(!forgot)}
            >
              {t("Forgot your password?")}
            </button>
            {forgot && (
              <p id="signin-forgot-help" className="hint" role="status">
                {t(
                  "Ask your administrator to reset it. They give you a temporary password, and you choose a new one when you sign in.",
                )}
              </p>
            )}
          </div>
          {error && <Notice tone="danger" title={error} />}
          <Button type="submit" variant="primary" size="lg" loading={busy}>
            {t("Sign in")}
          </Button>
          {passkeysSupported() && (
            <Button icon="key" size="lg" loading={asking} onClick={() => void passwordless()}>
              {t("Sign in with a passkey")}
            </Button>
          )}
          {microsoft && host.allows("microsoft") && (
            <>
              <Button
                icon="shield-check"
                size="lg"
                loading={leaving}
                onClick={() => {
                  setLeaving(true);
                  rememberPlace();
                  location.assign(SSO_START);
                }}
              >
                {t("Sign in with Microsoft")}
              </Button>
              <span className="sr-only" role="status" aria-live="polite">
                {leaving ? t("Going to Microsoft") : ""}
              </span>
            </>
          )}
          <SignInNotes microsoft={microsoft} />
          <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
        </form>
      )}
    </main>
  );
}

import { useEffect, useId, useRef, useState, type FocusEvent } from "react";
import { Button, Checkbox, ConfirmDialog, Dialog, Input, t, useWorded } from "@obop/ui";
import { call } from "./api.ts";
import type { Notify } from "./SecurityAccount.tsx";
import { serverMessage } from "./serverMessage.ts";
import { isGuid, type SsoState } from "./SsoAdmin.tsx";

/* People: one person's Microsoft sign-in (D91). Bind the account to the person's object id at
   Microsoft, remove the binding, and make the account sign in with Microsoft only. The commands are
   `user_account.bind_sso`, `unbind_sso` and `set_sso_only`; the server decides every rule (a provider
   first, a person who has signed in once, two other administrators with a password and a second
   step) and this dialog shows its sentence when it refuses. */

/** What the People list knows of a person's Microsoft binding. */
export type PersonSso = { name: string; sso_only?: boolean; sso_oid?: string | null };

/** The server's refusal worded in the language shown now (D9), or the fallback when it said nothing. */
function refusal(r: { kind: string; message?: string }, fallback: () => string): string {
  return (r.kind === "error" && r.message && serverMessage(r.message)) || fallback();
}

export function MicrosoftDialog({
  token,
  person,
  sso,
  me,
  notify,
  onClose,
  onDone,
  onSignedOut,
}: {
  token: string;
  person: PersonSso;
  sso: SsoState;
  /** The name of the signed-in administrator: a change of their own account says so. */
  me?: string;
  notify: Notify;
  onClose: () => void;
  /** After a change: People reloads. */
  onDone: () => void;
  /** After a change that ends the signed-in administrator's own sessions: the app's own sign-out. */
  onSignedOut?: () => void;
}) {
  const [oid, setOid] = useState("");
  const [asked, setAsked] = useState(false);
  const [left, setLeft] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [busy, setBusy] = useState(false);
  // Kept as how to word it, so a switch of the language words it again (D9).
  const [error, setError] = useWorded();
  // The question before a change that ends the person's sessions.
  const [confirm, setConfirm] = useState<"only" | "unbind" | null>(null);
  const form = useRef<HTMLFormElement>(null);
  const reasonId = useId();
  const inflight = useRef(false);
  const own = !!me && me.trim().toLowerCase() === person.name.trim().toLowerCase();
  const bound = typeof person.sso_oid === "string" && person.sso_oid !== "";
  const only = person.sso_only === true;
  const set = sso.kind === "set";
  const oidError =
    (asked || left) && !isGuid(oid)
      ? oid.includes("@")
        ? t("This is an e-mail address, not an object ID.")
        : t("This is not an object ID: it has 32 letters and digits in five groups.")
      : undefined;
  // Turning Microsoft only on needs a provider that is on; the server refuses it otherwise.
  const providerOff = sso.kind === "none" || (sso.kind === "set" && !sso.provider.enabled);
  // ...and two OTHER administrators with a password and a second step (the server counts others).
  const others =
    sso.kind === "set"
      ? sso.provider.break_glass
          .split(",")
          .map((n) => n.trim().toLowerCase())
          .filter((n) => n !== "" && n !== person.name.trim().toLowerCase()).length
      : 0;
  const tooFew = sso.kind === "set" && others < 2;
  useEffect(() => {
    if (attempt > 0) form.current?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }, [attempt]);
  const leave = (e: FocusEvent<HTMLInputElement>) => {
    if (oid.trim() !== "" && !(e.relatedTarget instanceof HTMLButtonElement)) setLeft(true);
  };

  /** One command, then the toast and the reload; the server's sentence stays in the dialog. */
  async function run(
    command: string,
    body: object,
    done: string,
    failed: () => string,
    endsOwnSessions = false,
  ): Promise<boolean> {
    // A second Enter in the same moment must not send a second request.
    if (inflight.current) return false;
    inflight.current = true;
    setBusy(true);
    setError(null);
    const r = await call(token, `user_account.${command}`, { id: person.name, ...body });
    inflight.current = false;
    setBusy(false);
    if (r.kind === "ok") {
      notify(done);
      // The server ended this administrator's own sessions: the next request would answer 401.
      // Leave the way "Sign out everywhere" does, and do not reload a list nobody may read now.
      if (own && endsOwnSessions && onSignedOut) onSignedOut();
      else onDone();
      return true;
    }
    setError(() => refusal(r, failed));
    return false;
  }

  async function bind() {
    setAsked(true);
    if (!isGuid(oid)) {
      setAttempt((a) => a + 1);
      return;
    }
    const ok = await run(
      "bind_sso",
      { oid: oid.trim() },
      t("{user} is bound to Microsoft.", { user: person.name }),
      () => t("Could not bind the account."),
    );
    if (ok) onClose();
  }

  async function unbind() {
    const ok = await run(
      "unbind_sso",
      {},
      t("The Microsoft binding of {user} was removed.", { user: person.name }),
      () => t("Could not remove the binding."),
      true,
    );
    if (ok) onClose();
  }

  async function setOnly(on: boolean) {
    const ok = await run(
      "set_sso_only",
      { sso_only: on },
      on
        ? t("{user} now signs in with Microsoft only.", { user: person.name })
        : t("{user} can use the password again and must choose a new one at the next sign-in.", {
            user: person.name,
          }),
      () => t("Could not save."),
      on,
    );
    setConfirm(null);
    if (ok) onClose();
  }

  const title = t("Microsoft sign-in for {user}", { user: person.name });

  // Nothing to bind to: the provider comes first. (A binding that is there stays manageable.)
  if (!bound && !set)
    return (
      <Dialog
        title={title}
        size="md"
        onClose={busy ? () => {} : onClose}
        footer={<Button onClick={onClose}>{t("Close")}</Button>}
      >
        <p>
          {sso.kind === "none"
            ? t("Set up Microsoft sign-in first.")
            : sso.kind === "loading"
              ? t("Loading…")
              : t("Microsoft sign-in could not be loaded. Try again.")}
        </p>
      </Dialog>
    );

  if (!bound)
    return (
      <Dialog
        title={title}
        size="md"
        onClose={busy ? () => {} : onClose}
        footer={
          <>
            <Button onClick={onClose} disabled={busy}>
              {t("Cancel")}
            </Button>
            <Button variant="primary" loading={busy} onClick={() => void bind()}>
              {t("Bind")}
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
            void bind();
          }}
        >
          <Input
            label={t("Object ID")}
            hint={t(
              "Copy it from the person's page in the Microsoft Entra admin centre (Users, the person, Object ID). It looks like 00000000-0000-0000-0000-000000000000.",
            )}
            value={oid}
            onChange={(e) => setOid(e.target.value)}
            onBlur={leave}
            error={oidError}
            autoComplete="off"
            spellCheck={false}
            required
          />
          {/* A binding gives every agent token of the account an end (N18): the server sets it
              on the tokens there are now and on each one made later. */}
          <p className="hint">
            {t(
              "Once bound, the agent tokens of {user} end after 90 days at the latest: the ones they have now 90 days from today, each new one 90 days after it is made.",
              { user: person.name },
            )}
          </p>
          {error && (
            <div className="confirm-error" role="alert">
              {error}
            </div>
          )}
        </form>
      </Dialog>
    );

  return (
    <>
      <Dialog
        title={title}
        size="md"
        onClose={busy ? () => {} : onClose}
        footer={
          <Button onClick={onClose} disabled={busy}>
            {t("Close")}
          </Button>
        }
      >
        <div className="stack">
          <p className="sso-bound">{t("Bound to {oid}", { oid: person.sso_oid ?? "" })}</p>
          <Checkbox
            label={t("Microsoft only")}
            hint={`${t(
              "The account then refuses its password and its passkeys. The person must have signed in with the Microsoft button once, and two other administrators must keep a password and a second step. Ends the person's sessions.",
            )}${providerOff && !only ? ` ${t("Turn Microsoft sign-in on first (Customize, Security).")}` : ""}${tooFew && !only ? ` ${t("At least two other administrators with a password and a second step are needed before an account can be made Microsoft only.")}` : ""}`}
            checked={only}
            disabled={busy || ((providerOff || tooFew) && !only)}
            onChange={(e) => (e.target.checked ? setConfirm("only") : void setOnly(false))}
          />
          {error && confirm !== "unbind" && (
            <div className="confirm-error" role="alert">
              {error}
            </div>
          )}
          <div className="stack">
            <div className="actionrow">
              <Button
                disabled={busy || only}
                aria-describedby={only ? reasonId : undefined}
                onClick={() => {
                  setError(null);
                  setConfirm("unbind");
                }}
              >
                {t("Remove binding")}
              </Button>
            </div>
            {only && (
              <p className="hint" id={reasonId}>
                {t("Turn off Microsoft only before removing the binding.")}
              </p>
            )}
          </div>
        </div>
      </Dialog>
      {confirm === "only" && (
        <ConfirmDialog
          title={t("Make {user} sign in with Microsoft only?", { user: person.name })}
          confirmLabel={t("Make Microsoft only")}
          busy={busy}
          onConfirm={() => void setOnly(true)}
          onClose={() => setConfirm(null)}
        >
          <div className="stack">
            <p>
              {t(
                "The password and passkeys of {user} are refused from now on. Their sessions and phone sign-ins end.",
                { user: person.name },
              )}
            </p>
            {/* D91 section 5 and N5: turning it on ends no agent token and no MCP grant, and a
                grant made while it is on lasts 30 days at most (phone or MCP client). */}
            <p>
              {t(
                "Agent tokens and connected apps stay valid. An app they connect from now on lasts 30 days at most.",
              )}
            </p>
            {own && <p>{t("This is your own account: you are signed out at once.")}</p>}
          </div>
        </ConfirmDialog>
      )}
      {confirm === "unbind" && (
        <ConfirmDialog
          title={t("Remove the Microsoft binding of {user}?", { user: person.name })}
          confirmLabel={t("Remove binding")}
          busy={busy}
          error={error}
          onConfirm={() => void unbind()}
          onClose={() => setConfirm(null)}
        >
          <div className="stack">
            <p>
              {t(
                "Their sessions and phone sign-ins end. Agent tokens and connected apps stay valid until the end date each one already has. Their password still works.",
              )}
            </p>
            {own && <p>{t("This is your own account: you are signed out at once.")}</p>}
          </div>
        </ConfirmDialog>
      )}
    </>
  );
}

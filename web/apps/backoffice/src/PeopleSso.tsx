import { useState } from "react";
import { Button, Checkbox, Dialog, Input, t } from "@obop/ui";
import { call } from "./api.ts";
import type { Notify } from "./SecurityAccount.tsx";
import { isGuid, type SsoState } from "./SsoAdmin.tsx";

/* People: one person's Microsoft sign-in (D91). Bind the account to the person's object id at
   Microsoft, remove the binding, and make the account sign in with Microsoft only. The commands are
   `user_account.bind_sso`, `unbind_sso` and `set_sso_only`; the server decides every rule (a provider
   first, a person who has signed in once, two other administrators with a password and a second
   step) and this dialog shows its sentence when it refuses. */

/** What the People list knows of a person's Microsoft binding. */
export type PersonSso = { name: string; sso_only?: boolean; sso_oid?: string | null };

function refusal(r: { kind: string; message?: string }, fallback: string): string {
  return (r.kind === "error" && r.message) || fallback;
}

export function MicrosoftDialog({
  token,
  person,
  sso,
  notify,
  onClose,
  onDone,
}: {
  token: string;
  person: PersonSso;
  sso: SsoState;
  notify: Notify;
  onClose: () => void;
  /** After a change: People reloads. */
  onDone: () => void;
}) {
  const [oid, setOid] = useState("");
  const [asked, setAsked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bound = typeof person.sso_oid === "string" && person.sso_oid !== "";
  const only = person.sso_only === true;
  const set = sso.kind === "set";
  const oidError =
    asked && !isGuid(oid) ? t("The object ID is a GUID, not an e-mail address.") : undefined;

  /** One command, then the toast and the reload; the server's sentence stays in the dialog. */
  async function run(
    command: string,
    body: object,
    done: string,
    failed: string,
  ): Promise<boolean> {
    setBusy(true);
    setError(null);
    const r = await call(token, `user_account.${command}`, { id: person.name, ...body });
    setBusy(false);
    if (r.kind === "ok") {
      notify(done);
      onDone();
      return true;
    }
    setError(refusal(r, failed));
    return false;
  }

  async function bind() {
    setAsked(true);
    if (!isGuid(oid)) return;
    const ok = await run(
      "bind_sso",
      { oid: oid.trim() },
      t("{user} is bound to Microsoft.", { user: person.name }),
      t("Could not bind the account."),
    );
    if (ok) onClose();
  }

  async function unbind() {
    const ok = await run(
      "unbind_sso",
      {},
      t("The Microsoft binding of {user} was removed.", { user: person.name }),
      t("Could not remove the binding."),
    );
    if (ok) onClose();
  }

  async function setOnly(on: boolean) {
    const ok = await run(
      "set_sso_only",
      { sso_only: on },
      on
        ? t("{user} now signs in with Microsoft only.", { user: person.name })
        : t("{user} can use the password again.", { user: person.name }),
      t("Could not save."),
    );
    if (ok) onClose();
  }

  const title = t("Microsoft sign-in for {user}", { user: person.name });

  // Nothing to bind to: the provider comes first. (A binding that is there stays manageable.)
  if (!bound && !set)
    return (
      <Dialog
        title={title}
        size="md"
        onClose={onClose}
        footer={<Button onClick={onClose}>{t("Close")}</Button>}
      >
        <p>
          {sso.kind === "none"
            ? t("Set up Microsoft sign-in first.")
            : t("Microsoft sign-in could not be loaded. Try again.")}
        </p>
      </Dialog>
    );

  if (!bound)
    return (
      <Dialog
        title={title}
        size="md"
        onClose={onClose}
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
              "The person's Object ID in the Microsoft Entra admin centre (Users, the person, Object ID). A GUID, not an e-mail address.",
            )}
            value={oid}
            onChange={(e) => setOid(e.target.value)}
            error={oidError}
            autoComplete="off"
            spellCheck={false}
            required
          />
          {error && (
            <div className="confirm-error" role="alert">
              {error}
            </div>
          )}
        </form>
      </Dialog>
    );

  return (
    <Dialog
      title={title}
      size="md"
      onClose={onClose}
      footer={<Button onClick={onClose}>{t("Close")}</Button>}
    >
      <div className="stack">
        <p className="sso-bound">{t("Bound to {oid}", { oid: person.sso_oid ?? "" })}</p>
        <Checkbox
          label={t("Microsoft only")}
          hint={t(
            "The account then refuses its password and its passkeys. The person must have signed in with the Microsoft button once, and two other administrators must keep a password and a second step. Ends the person's sessions.",
          )}
          checked={only}
          disabled={busy}
          onChange={(e) => void setOnly(e.target.checked)}
        />
        <div className="stack">
          <div className="actionrow">
            <Button disabled={busy || only} onClick={() => void unbind()}>
              {t("Remove binding")}
            </Button>
          </div>
          {only && (
            <p className="hint">{t("Turn off Microsoft only before removing the binding.")}</p>
          )}
        </div>
        {error && (
          <div className="confirm-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}

import { useEffect, useId, useRef, useState } from "react";
import { Badge, Button, Dialog, Input, Notice, Table, Tooltip, t } from "@obop/ui";
import { search, type FieldMeta, type Row } from "./api.ts";
import { useData } from "./data.tsx";
import { instantText } from "./format.ts";
import { useLeaveHold } from "./leaveGuard.ts";
import { serverMessage } from "./serverMessage.ts";
import {
  LINK_CREATE,
  LINK_MODEL,
  LINK_UPDATE,
  canRevokeStatus,
  closeAttempt,
  copiedWholeLink,
  copyText,
  linkGate,
  linkStatus,
  madeLink,
  revokeBody,
  selectAll,
  type LinkStatus,
  type MadeLink,
} from "./signLinks.ts";
import { isDocumentRequest } from "./workflow.ts";

/* D92 (docs/SIGN.md): on a sign request that has been sent, a person with the right to make links
   (the write group of the link model, `sign.manager`) presses "Create signing link". The server
   makes a one-time link and keeps only a hash of it, so the link is shown here once, in a dialog,
   with a Copy button. Closing the dialog drops it; a lost link is replaced by making another. So
   the dialog is hard to close by accident: Escape, a click outside, the X and Done ask first until
   the link has been copied (with the button, or by hand with the whole link selected), and focus
   starts on Copy. Leaving the page by other ways asks too (Back, a reload, the palette, closing the
   panel: `leaveGuard.ts`), also while the link is still being made. Nothing of the link is stored in
   the browser, written to a toast or kept in a list. */

/**
 * Said in the panel of a request about a document that is not signed yet: the signer signs it through
 * the link, so there is no step to mark it signed by hand (the server refuses it, and the panel has
 * no button for it).
 */
export function SignByLinkNote({ row }: { row: Row }) {
  const state = String(row.state ?? "");
  if (!isDocumentRequest(row) || (state !== "draft" && state !== "sent")) return null;
  return (
    <p className="hint sign-by-link">
      {t(
        "This request is about a document. The signer signs it through a signing link that you create here after sending it. It cannot be marked signed by hand.",
      )}
    </p>
  );
}

/** The button of the record panel. Renders nothing where the person may not make links. */
export function SignLinkAction({
  row,
  fields,
  disabled,
}: {
  row: Row;
  fields: FieldMeta[];
  disabled?: boolean;
}) {
  const d = useData();
  const link = d.models.get(LINK_MODEL)?.model;
  const [busy, setBusy] = useState(false);
  const [made, setMade] = useState<MadeLink | null>(null);
  // The server makes the link when it is asked, so leaving before the answer would lose it.
  useLeaveHold(
    busy
      ? {
          message: t(
            "The signing link is still being made. If you leave now, you will not see it and will have to make another one. Leave anyway?",
          ),
          leave: t("Leave anyway"),
          stay: t("Stay"),
        }
      : null,
  );
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  // Not installed (an older package) or not allowed for this person: no button at all. The generic
  // New of the model is switched off (`via_action`, api.ts), so the right is read from there.
  if (!link?.via_action) return null;

  const gate = linkGate(row, fields);
  const blocked = !gate.ok || busy || !!disabled;

  async function create() {
    if (blocked) return;
    setBusy(true);
    const r = await d.run(LINK_CREATE, { request_id: row.id });
    if (!alive.current) {
      // The person left (and said so) before the answer: the link exists and nobody has it.
      if (r.kind === "ok")
        d.notify(
          t(
            "A signing link was made for this request, but you left before it was shown. It cannot be shown again. Open the request, revoke that link and make another one.",
          ),
          "error",
        );
      return;
    }
    setBusy(false);
    if (r.kind === "ok") {
      const m = madeLink(r.value, location.origin);
      if (m) setMade(m);
      else d.notify(t("The server did not return a link."), "error");
    } else if (r.kind === "error") d.notify(serverMessage(r.message), "error");
  }

  const button = (
    <Button
      key="sign-link"
      title={
        gate.ok
          ? t("Makes a one-time link that the signer opens without an account. It is shown once.")
          : undefined
      }
      aria-disabled={blocked ? true : undefined}
      loading={busy}
      onClick={() => void create()}
    >
      {t("Create signing link")}
    </Button>
  );
  return (
    <>
      {gate.ok ? (
        button
      ) : (
        <Tooltip label={gate.reason} side="top">
          {button}
        </Tooltip>
      )}
      {made && (
        <SignLinkDialog
          made={made}
          signer={String(row.signer_name ?? "")}
          onClose={() => setMade(null)}
        />
      )}
    </>
  );
}

/** The dialog: what the person has done with the link, around the body. */
export function SignLinkDialog({
  made,
  signer,
  onClose,
}: {
  made: MadeLink;
  signer: string;
  onClose: () => void;
}) {
  const body = useRef<HTMLDivElement>(null);
  const field = () =>
    body.current?.querySelector<HTMLInputElement>(".sign-link-field input") ?? null;
  const noteId = useId();
  const [copied, setCopied] = useState<"yes" | "no" | null>(null);
  // The link was copied at least once (the button, or Ctrl+C in the field). Until then the dialog
  // does not close on Escape, a click outside, the X or Done: it asks first.
  const [kept, setKept] = useState(false);
  const [leaving, setLeaving] = useState(false);
  // Leaving the page by another way (Back, a reload, the palette, closing the panel) asks until then.
  useLeaveHold(
    kept
      ? null
      : {
          message: t(
            "The signing link has not been copied. If you leave now, it is lost and you will have to make another one. Leave anyway?",
          ),
          leave: t("Leave without the link"),
          stay: t("Stay and copy it"),
        },
  );
  const ask = () => {
    const step = closeAttempt(kept, leaving);
    if (step.close) onClose();
    else setLeaving(step.leaving);
  };
  useEffect(() => {
    if (leaving) body.current?.querySelector<HTMLElement>("[data-copy]")?.focus();
  }, [leaving]);
  return (
    <Dialog title={t("Signing link")} size="md" onClose={ask} describedBy={noteId}>
      <div ref={body}>
        <SignLinkBody
          made={made}
          signer={signer}
          copied={copied}
          leaving={leaving && !kept}
          noteId={noteId}
          onCopy={() => {
            void copyText(made.href, field()).then((ok) => {
              setCopied(ok ? "yes" : "no");
              if (ok) {
                setKept(true);
                setTimeout(() => setCopied(null), 1500);
              }
            });
          }}
          onFieldCopied={(input) => {
            if (copiedWholeLink(input, made.href)) setKept(true);
          }}
          onDone={ask}
          onCloseAnyway={onClose}
        />
      </div>
    </Dialog>
  );
}

/** The link, shown once. Everything the person needs to do and to know is here. */
export function SignLinkBody({
  made,
  signer,
  copied,
  leaving = false,
  noteId,
  onCopy,
  onFieldCopied,
  onDone,
  onCloseAnyway,
}: {
  made: MadeLink;
  signer: string;
  copied: "yes" | "no" | null;
  /** The person tried to close it before copying it: ask whether they really mean it. */
  leaving?: boolean;
  /** The id the dialog points `aria-describedby` at, so the warning is read with its title. */
  noteId?: string;
  onCopy: () => void;
  /** A copy event in the field; the dialog decides whether it was the whole link. */
  onFieldCopied?: (input: HTMLInputElement) => void;
  onDone: () => void;
  onCloseAnyway?: () => void;
}) {
  return (
    <div className="stack sign-link">
      <div id={noteId}>
        <Notice tone="warning" title={t("Shown once")}>
          {t(
            "Anyone who has this link can sign for {name}. It cannot be shown again. If you lose it, create another link for this request. obop sends no email, so give the link to the signer yourself.",
            { name: signer },
          )}
        </Notice>
      </div>
      <Input
        label={t("Signing link")}
        className="sign-link-field"
        value={made.href}
        readOnly
        spellCheck={false}
        autoComplete="off"
        data-testid="sign-link"
        onFocus={(e) => selectAll(e.currentTarget)}
        onCopy={(e) => onFieldCopied?.(e.currentTarget)}
        hint={
          made.expiresAt
            ? t("The link works once and stops working on {date}.", {
                date: instantText(made.expiresAt),
              })
            : undefined
        }
      />
      {copied === "no" && (
        <p className="hint" role="status">
          {t("Could not copy automatically. Select the link and copy it yourself.")}
        </p>
      )}
      {/* One polite live region that is always there, so "Copied" is read out when it appears. */}
      <span className="sr-only" role="status">
        {copied === "yes" ? t("Copied") : ""}
      </span>
      {leaving && (
        <Notice tone="danger" title={t("Close without copying?")}>
          {t(
            "You have not copied the link. If you close this window now, the link is lost and you will have to make another one.",
          )}
        </Notice>
      )}
      <div className="actionrow">
        <Button
          data-autofocus
          data-copy
          variant={leaving ? "primary" : "secondary"}
          onClick={onCopy}
        >
          {copied === "yes" ? t("Copied") : t("Copy")}
        </Button>
        {leaving ? (
          <Button variant="danger" onClick={onCloseAnyway}>
            {t("Close without copying")}
          </Button>
        ) : (
          <Button variant="primary" onClick={onDone}>
            {t("Done")}
          </Button>
        )}
      </div>
    </div>
  );
}

const TONE = {
  active: "success",
  used: "info",
  revoked: "neutral",
  expired: "warning",
  closed: "neutral",
} as const;

/** The words of a status. Written where it is drawn, so they follow a change of language. */
function statusText(s: LinkStatus): string {
  return s === "active"
    ? t("Active")
    : s === "used"
      ? t("Used")
      : s === "revoked"
        ? t("Revoked")
        : s === "closed"
          ? t("Request closed")
          : t("Expired");
}

/**
 * The links made for one request, with their status (as the door will see it, `linkStatus`) and a
 * Revoke button for every link that is not used or revoked yet. A link is never shown again (the
 * server keeps a hash); the first characters of its id are only a way to tell two links apart and
 * to find the line in the security log.
 */
export function SignLinkTable({
  rows,
  request,
  nowMs,
  canRevoke,
  busy,
  onRevoke,
}: {
  rows: Row[];
  /** The request the links belong to: its state and its `expires_on` end the links too. */
  request?: Row;
  nowMs: number;
  canRevoke: boolean;
  busy: string | null;
  onRevoke: (row: Row) => void;
}) {
  return (
    <section className="sign-links" aria-label={t("Signing links")}>
      <h3 className="overline">{t("Signing links")}</h3>
      <Table caption={t("Signing links")} dense>
        <thead>
          <tr>
            <th scope="col">{t("Status")}</th>
            <th scope="col">{t("Expires")}</th>
            <th scope="col">{t("Used or revoked")}</th>
            <th scope="col" className="sr-only">
              {t("Actions")}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => {
            const st = linkStatus(r, nowMs, request);
            const done = String(r.used_at || r.revoked_at || "");
            return (
              <tr key={r.id}>
                <td>
                  <Badge tone={TONE[st]}>{statusText(st)}</Badge>{" "}
                  <code className="hint">{r.id.slice(0, 8)}</code>
                </td>
                <td>{r.expires_at ? instantText(r.expires_at) : ""}</td>
                <td>{done ? instantText(done) : ""}</td>
                <td>
                  {canRevokeStatus(st) && canRevoke && (
                    <Button
                      size="sm"
                      variant="ghost"
                      className="danger-text"
                      disabled={busy !== null}
                      loading={busy === r.id}
                      aria-label={t("Revoke signing link {id}", { id: r.id.slice(0, 8) })}
                      onClick={() => onRevoke(r)}
                    >
                      {t("Revoke")}
                    </Button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </Table>
    </section>
  );
}

/** Loads the links of one request and refreshes after every change the back office makes. */
export function SignLinks({ request }: { request: Row }) {
  const requestId = request.id;
  const d = useData();
  const link = d.models.get(LINK_MODEL)?.model;
  const [rows, setRows] = useState<Row[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => {
    if (!link) return;
    let live = true;
    void search(d.token, LINK_MODEL, {
      filter: [{ field: "request_id", op: "eq", value: requestId }],
      order: [{ field: "expires_at", dir: "desc" }],
      limit: 50,
    }).then((r) => {
      if (live) setRows("error" in r ? [] : r.rows);
    });
    return () => {
      live = false;
    };
  }, [d.token, requestId, d.mutations, !!link]);
  if (!link || !rows || rows.length === 0) return null;

  async function revoke(row: Row) {
    const ok = await d.confirm(
      t("Revoke this signing link? Whoever holds it can no longer sign with it."),
      t("Revoke"),
    );
    if (!ok) return;
    setBusy(row.id);
    const r = await d.run(LINK_UPDATE, revokeBody(row.id, new Date()));
    setBusy(null);
    if (r.kind === "ok") d.notify(t("The signing link was revoked"));
    else if (r.kind === "error") d.notify(serverMessage(r.message), "error");
  }

  return (
    <SignLinkTable
      rows={rows}
      request={request}
      nowMs={Date.now()}
      canRevoke={!!link.can_update}
      busy={busy}
      onRevoke={(row) => void revoke(row)}
    />
  );
}

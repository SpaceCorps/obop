import { useEffect, useRef, useState, type RefObject } from "react";
import { Badge, Button, Dialog, Input, Notice, Table, Tooltip, t } from "@obop/ui";
import { search, type FieldMeta, type Row } from "./api.ts";
import { useData } from "./data.tsx";
import { instantText } from "./format.ts";
import { serverMessage } from "./serverMessage.ts";
import {
  LINK_CREATE,
  LINK_MODEL,
  LINK_UPDATE,
  copyText,
  linkGate,
  linkStatus,
  madeLink,
  revokeBody,
  selectAll,
  type LinkStatus,
  type MadeLink,
} from "./signLinks.ts";

/* D92 (docs/SIGN.md): on a sign request that has been sent, a person with the right to make links
   (the write group of the link model, `sign.manager`) presses "Create signing link". The server
   makes a one-time link and keeps only a hash of it, so the link is shown here once, in a dialog,
   with a Copy button. Closing the dialog drops it; a lost link is replaced by making another.
   Nothing of the link is stored in the browser, written to a toast or kept in a list. */

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
  // Not installed (an older package) or not allowed for this person: no button at all.
  if (!link?.can_create) return null;

  const gate = linkGate(row, fields);
  const blocked = !gate.ok || busy || !!disabled;

  async function create() {
    if (blocked) return;
    setBusy(true);
    const r = await d.run(LINK_CREATE, { request_id: row.id });
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

/** The dialog: the state of the copy button around the body. */
export function SignLinkDialog({
  made,
  signer,
  onClose,
}: {
  made: MadeLink;
  signer: string;
  onClose: () => void;
}) {
  const field = useRef<HTMLInputElement>(null);
  const [copied, setCopied] = useState<"yes" | "no" | null>(null);
  return (
    <Dialog title={t("Signing link")} size="md" onClose={onClose}>
      <SignLinkBody
        made={made}
        signer={signer}
        copied={copied}
        field={field}
        onCopy={() => {
          void copyText(made.href, field.current).then((ok) => {
            setCopied(ok ? "yes" : "no");
            if (ok) setTimeout(() => setCopied(null), 1500);
          });
        }}
        onDone={onClose}
      />
    </Dialog>
  );
}

/** The link, shown once. Everything the person needs to do and to know is here. */
export function SignLinkBody({
  made,
  signer,
  copied,
  field,
  onCopy,
  onDone,
}: {
  made: MadeLink;
  signer: string;
  copied: "yes" | "no" | null;
  field?: RefObject<HTMLInputElement | null>;
  onCopy: () => void;
  onDone: () => void;
}) {
  return (
    <div className="stack sign-link">
      <Notice tone="warning" title={t("Shown once")}>
        {t(
          "Anyone who has this link can sign for {name}. It cannot be shown again. If you lose it, create another link for this request. obop sends no email, so give the link to the signer yourself.",
          { name: signer },
        )}
      </Notice>
      <Input
        ref={field}
        label={t("Signing link")}
        className="sign-link-field"
        value={made.href}
        readOnly
        spellCheck={false}
        autoComplete="off"
        data-testid="sign-link"
        onFocus={() => selectAll(field?.current ?? null)}
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
      <div className="actionrow">
        <Button onClick={onCopy}>{copied === "yes" ? t("Copied") : t("Copy")}</Button>
        <Button variant="primary" onClick={onDone}>
          {t("Done")}
        </Button>
      </div>
    </div>
  );
}

const TONE = { active: "success", used: "info", revoked: "neutral", expired: "warning" } as const;

/** The words of a status. Written where it is drawn, so they follow a change of language. */
function statusText(s: LinkStatus): string {
  return s === "active"
    ? t("Active")
    : s === "used"
      ? t("Used")
      : s === "revoked"
        ? t("Revoked")
        : t("Expired");
}

/**
 * The links made for one request, with their status and a Revoke button for an active one. A link
 * is never shown again (the server keeps a hash); the first characters of its id are only a way to
 * tell two links apart and to find the line in the security log.
 */
export function SignLinkTable({
  rows,
  nowMs,
  canRevoke,
  busy,
  onRevoke,
}: {
  rows: Row[];
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
            const st = linkStatus(r, nowMs);
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
                  {st === "active" && canRevoke && (
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
export function SignLinks({ requestId }: { requestId: string }) {
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
      nowMs={Date.now()}
      canRevoke={!!link.can_update}
      busy={busy}
      onRevoke={(row) => void revoke(row)}
    />
  );
}

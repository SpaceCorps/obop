// The journal entry of a posted invoice (owner ruling of 2026-11-17: a posted invoice books its
// entry). `invoice_posting.get` says whether the invoice has one; when it has none (booking on post
// was off or not set up when it was posted, or it was posted before), "Book to ledger" runs
// `invoice_posting.create_move` with the company's booking settings and the issue date. Shown only
// where the accounting package is installed and the person may read the ledger.
import { useEffect, useState } from "react";
import { Button, Notice, t } from "@obop/ui";
import { call } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { serverMessage } from "../../serverMessage.ts";

const MOVE = "accounting_move";

type Link = { move_id: string; move_state: string; move_number: string };

export function LedgerPanel({ row, busy }: RecordExtProps) {
  const d = useData();
  const [link, setLink] = useState<Link | null | undefined>(undefined);
  const [running, setRunning] = useState(false);
  const ledger = d.models.has(MOVE);
  const posted = row.state === "posted";
  useEffect(() => {
    let live = true;
    setLink(undefined);
    if (!ledger || !posted) return;
    void call(d.token, "invoice_posting.get", { invoice: row.id }).then((r) => {
      if (!live) return;
      // Refused (no entry) and forbidden (no ledger group) look alike here: no entry to show.
      setLink(r.kind === "ok" ? (r.value as Link) : null);
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id, ledger, posted, d.mutations]);
  if (!ledger || !posted || link === undefined) return null;
  const href = link ? d.linkTo(MOVE, { id: link.move_id }) : null;
  if (link)
    return (
      <Notice
        tone={link.move_state === "cancelled" ? "warning" : "success"}
        action={
          href ? (
            <a className="btn small" href={href}>
              {t("Open entry")}
            </a>
          ) : undefined
        }
      >
        {link.move_state === "posted"
          ? t("Booked to the ledger as entry {number}.", { number: link.move_number })
          : link.move_state === "cancelled"
            ? t("Its journal entry was cancelled.")
            : t("Booked to the ledger as a draft entry. An approver posts it.")}
      </Notice>
    );
  const book = async () => {
    setRunning(true);
    const r = await d.run("invoice_posting.create_move", { invoice: row.id });
    setRunning(false);
    if (r.kind === "approval")
      return d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
    if (r.kind !== "ok") return d.notify(serverMessage(r.message), "error");
    d.notify(t("Booked as a draft entry"), "ok");
    await d.changed(MOVE);
  };
  return (
    <Notice
      tone="info"
      action={
        <Button size="sm" loading={running} disabled={busy} onClick={() => void book()}>
          {t("Book to ledger")}
        </Button>
      }
    >
      {t("Not booked to the ledger yet.")}
    </Notice>
  );
}

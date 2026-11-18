// "Undo booking" on a bank transaction that `bank_matching.book` booked (docs/ISSUES.md Q11): the
// server cancels the draft entry, unmatches the transaction, frees the item it paid and deletes the
// link in one call (`bank_matching.unbook`), so the invoice can be paid again. Shown only while the
// booking exists, its entry is still a draft and the transaction is not reconciled.
import { useEffect, useState } from "react";
import { Button, t } from "@obop/ui";
import { call } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { serverMessage } from "../../serverMessage.ts";

const MOVE = "accounting_move";

export function UnbookButton({ row, busy }: RecordExtProps) {
  const d = useData();
  const [draft, setDraft] = useState(false);
  const [running, setRunning] = useState(false);
  useEffect(() => {
    let live = true;
    setDraft(false);
    if (row.state === "reconciled") return;
    void call(d.token, "bank_matching.get", { bank_line: row.id }).then((r) => {
      if (live && r.kind === "ok")
        setDraft((r.value as { move_state?: string }).move_state === "draft");
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id, row.state, d.mutations]);
  if (!draft) return null;
  const undo = async () => {
    const ok = await d.confirm(
      t(
        "Undo this booking? Its draft entry is cancelled and the transaction is unmatched, so the invoice can be paid again.",
      ),
      t("Undo booking"),
    );
    if (!ok) return;
    setRunning(true);
    const r = await d.run("bank_matching.unbook", { bank_line: row.id });
    setRunning(false);
    if (r.kind === "approval")
      return d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
    if (r.kind !== "ok") return d.notify(serverMessage(r.message), "error");
    d.notify(t("Booking undone"), "ok");
    await Promise.all([d.changed("accounting_bank_statement_line"), d.changed(MOVE)]);
  };
  return (
    <Button icon="undo" disabled={busy || running} loading={running} onClick={() => void undo()}>
      {t("Undo booking")}
    </Button>
  );
}

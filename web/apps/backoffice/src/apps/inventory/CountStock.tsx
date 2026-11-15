// "Count stock" on a stock item: type what is really on the shelf, and the difference is booked as
// a stock count move (kind `adjustment`) that is validated at once. The quantity itself is never
// typed in (it is read-only on the server); only moves change it.
import { useState } from "react";
import { fmt, t } from "@obop/ui";
import type { RecordExtProps } from "../../extensions.ts";
import { useData } from "../../data.tsx";
import { serverMessage } from "../../serverMessage.ts";
import { Button, Dialog, Input } from "../../ui.tsx";
import { makeMove, mayValidate, MOVE } from "./moves.ts";
import "../supply.css";

export function CountStockAction({ row, busy }: RecordExtProps) {
  const d = useData();
  const [open, setOpen] = useState(false);
  const move = d.models.get(MOVE)?.model;
  // Counting writes a move: the person needs to be able to make one.
  if (!move?.can_create || !row.product_id || !row.location_id) return null;
  return (
    <>
      <Button variant="primary" icon="edit" disabled={busy} onClick={() => setOpen(true)}>
        {t("Count stock")}
      </Button>
      {open && <CountDialog row={row} onClose={() => setOpen(false)} />}
    </>
  );
}

function CountDialog({ row, onClose }: { row: RecordExtProps["row"]; onClose: () => void }) {
  const d = useData();
  const move = d.models.get(MOVE)?.model;
  const onHand = Number(row.quantity ?? 0);
  const [counted, setCounted] = useState(String(onHand));
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const n = /^\d+$/.test(counted.trim()) ? Number(counted.trim()) : null;
  const diff = n === null ? null : n - onHand;
  const product = d.refText("product", String(row.product_id));
  const where = d.refText("inventory_location", String(row.location_id));
  const lot = String(row.lot ?? "");

  async function submit() {
    if (n === null) {
      setError(t("Enter the number of units you counted"));
      return;
    }
    if (diff === 0) {
      d.notify(t("The count matches the stock; nothing changed."), "info");
      onClose();
      return;
    }
    setBusy(true);
    setError("");
    const body: Record<string, unknown> = {
      kind: "adjustment",
      origin: t("Stock count"),
      product_id: row.product_id,
      quantity: Math.abs(diff!),
      ...(diff! > 0 ? { to_location_id: row.location_id } : { from_location_id: row.location_id }),
    };
    const out = await makeMove(d.run, move, body);
    setBusy(false);
    if (out.kind === "error") {
      setError(serverMessage(out.message));
      return;
    }
    await d.changed("inventory_item");
    if (out.kind === "done") d.notify(t("Stock set to {count}", { count: fmt.integer(n) }), "ok");
    else d.notify(t("Count saved as a draft move. {why}", { why: out.why }), "info");
    onClose();
  }

  return (
    <Dialog
      title={t("Count stock")}
      size="sm"
      onClose={onClose}
      className="sup-dialog"
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={() => void submit()}>
            {t("Save count")}
          </Button>
        </>
      }
    >
      <form
        className="sup-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <p className="sup-what">
          <strong>{product}</strong>
          <span className="hint">
            {where}
            {lot ? ` · ${lot}` : ""}
          </span>
        </p>
        <dl className="sup-figures">
          <div>
            <dt>{t("On hand")}</dt>
            <dd>{fmt.integer(onHand)}</dd>
          </div>
          <div>
            <dt>{t("Difference")}</dt>
            <dd className={diff ? (diff > 0 ? "sup-up" : "sup-down") : undefined}>
              {diff === null ? "–" : diff > 0 ? `+${fmt.integer(diff)}` : fmt.integer(diff)}
            </dd>
          </div>
        </dl>
        <Input
          id="count-qty"
          label={t("Counted")}
          inputMode="numeric"
          autoFocus
          value={counted}
          aria-invalid={!!error || undefined}
          onChange={(e) => setCounted(e.currentTarget.value)}
          onFocus={(e) => e.currentTarget.select()}
        />
        <p className="hint">
          {mayValidate(move)
            ? t("The difference is booked as a stock count move, and the stock changes now.")
            : t("The difference is saved as a stock count move for a stock manager to validate.")}
        </p>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}

// Receiving a confirmed purchase order: the lines with what was ordered and what arrived, the
// location the goods go to, and one button. It runs the commands an agent runs, in order:
// `purchase_order.update` (the location, on its own), `purchase_order_line.update` (the units that
// arrived, where they differ), `purchase_order.receive` (one draft move per line, with the order
// number as its source document), then `inventory_move.validate` for each move when the person may
// validate, so the goods are in stock when the dialog closes.
import { useEffect, useMemo, useState } from "react";
import { fmt, t } from "@obop/ui";
import { search, type FieldMeta, type Row } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData, useRows } from "../../data.tsx";
import { toEdit, fromEdit, type Edit } from "../../edit.ts";
import { FieldInput } from "../../fields.tsx";
import { fieldTitle } from "../../labels.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Button, Dialog, Icon } from "../../ui.tsx";
import { firstLocation, LOCATION, mayValidate, MOVE, settleMoves } from "../inventory/moves.ts";
import "../supply.css";

export const ORDER = "purchase_order";
const LINE = "purchase_order_line";
const RECEIVE = `${ORDER}.receive`;

/** The "Receive goods" button of a confirmed order (it replaces the generic Receive). */
export function ReceiveAction({ model, row, busy }: RecordExtProps) {
  const [open, setOpen] = useState(false);
  const may = model.actions.some((a) => a.id === RECEIVE);
  if (!may || row.state !== "confirmed") return null;
  return (
    <>
      <Button variant="primary" icon="download" disabled={busy} onClick={() => setOpen(true)}>
        {t("Receive goods")}
      </Button>
      {open && <ReceiveDialog order={row} onClose={() => setOpen(false)} />}
    </>
  );
}

type Line = { row: Row; arrived: string };

function ReceiveDialog({ order, onClose }: { order: Row; onClose: () => void }) {
  const d = useData();
  const orderMeta = d.models.get(ORDER)?.model;
  const move = d.models.get(MOVE)?.model;
  const { rows: locations } = useRows(LOCATION);
  const locField = orderMeta?.fields.find((f) => f.name === "receive_location_id") as
    | FieldMeta
    | undefined;
  const [lines, setLines] = useState<Line[] | null>(null);
  const [loc, setLoc] = useState<Edit | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    void search(d.token, LINE, {
      filter: [{ field: "order_id", op: "eq", value: order.id }],
      limit: 500,
    }).then((r) => {
      if (!live) return;
      if ("error" in r) setError(serverMessage(r.error));
      else
        setLines(
          r.rows.map((row) => ({
            row,
            arrived: String(row.received_quantity ?? row.quantity ?? ""),
          })),
        );
    });
    return () => {
      live = false;
    };
  }, [d.token, order.id]);

  const fallbackLoc =
    String(order.receive_location_id ?? "") || firstLocation(locations, "receiving");
  const location = loc ?? (locField ? toEdit(locField, fallbackLoc || undefined) : "");
  const total = useMemo(
    () => (lines ?? []).reduce((n, l) => n + (/^\d+$/.test(l.arrived) ? Number(l.arrived) : 0), 0),
    [lines],
  );

  async function submit() {
    if (!lines) return;
    const bad = lines.find((l) => !/^\d+$/.test(l.arrived.trim()));
    if (bad) {
      setError(t("Enter the units that arrived for every line (0 when none did)"));
      return;
    }
    const where = locField ? fromEdit({ ...locField, required: true }, location) : null;
    if (!where || "error" in where) {
      setError(t("Choose where the goods are received"));
      return;
    }
    setBusy(true);
    setError("");
    const fail = (m: string) => {
      setBusy(false);
      setError(serverMessage(m));
    };
    // The location first, in its own call (a state move is never mixed with other edits).
    if (where.value !== (order.receive_location_id ?? null)) {
      const r = await d.run(`${ORDER}.update`, { id: order.id, receive_location_id: where.value });
      if (r.kind === "error") return fail(r.message);
    }
    for (const l of lines) {
      const arrived = Number(l.arrived.trim());
      const ordered = Number(l.row.quantity ?? 0);
      const stored = l.row.received_quantity;
      const want = arrived === ordered ? null : arrived;
      if ((stored ?? null) === want) continue;
      const r = await d.run(`${LINE}.update`, { id: l.row.id, received_quantity: want });
      if (r.kind === "error") return fail(r.message);
    }
    const r = await d.run(RECEIVE, { id: order.id });
    if (r.kind === "error") return fail(r.message);
    if (r.kind === "approval") {
      setBusy(false);
      d.notify(t("Receiving was proposed and waits for approval."), "info");
      onClose();
      return;
    }
    // The moves the receipt made: validated now when the person may, else left for a manager.
    const { waiting } = await settleMoves(d.run, d.token, move, String(order.number ?? ""));
    await d.changed(MOVE);
    setBusy(false);
    const number = String(order.number ?? "");
    d.notify(
      waiting === 0
        ? t("{order} received: {count} units in stock", {
            order: number,
            count: fmt.integer(total),
          })
        : t("{order} received: {count} stock moves wait for a stock manager", {
            order: number,
            count: String(waiting),
          }),
      waiting === 0 ? "ok" : "info",
    );
    onClose();
  }

  return (
    <Dialog
      title={t("Receive {order}", { order: String(order.number ?? "") })}
      onClose={onClose}
      className="sup-dialog sup-receive"
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={!lines?.length}
            onClick={() => void submit()}
          >
            {t("Receive {count} units", { count: fmt.integer(total) })}
          </Button>
        </>
      }
    >
      <form
        className="sup-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          if (e.target !== e.currentTarget) return;
          void submit();
        }}
      >
        <p className="hint">
          {t("Check what arrived. Change a number when fewer or more units came.")}
        </p>
        {lines === null && !error ? (
          <p className="hint" aria-busy="true">
            {t("Loading")}
          </p>
        ) : (
          <ul className="sup-lines" aria-label={t("Lines")}>
            {(lines ?? []).map((l, i) => {
              const ordered = Number(l.row.quantity ?? 0);
              const n = /^\d+$/.test(l.arrived) ? Number(l.arrived) : null;
              const id = `rcv-${l.row.id}`;
              const set = (v: string) =>
                setLines((x) => x!.map((y, j) => (j === i ? { ...y, arrived: v } : y)));
              return (
                <li key={l.row.id} className="sup-line">
                  <label className="sup-line-name" htmlFor={id}>
                    <strong>{d.refText("product", String(l.row.product_id ?? ""))}</strong>
                    <span className="hint">
                      {t("Ordered {count}", { count: fmt.integer(ordered) })}
                      {n !== null && n !== ordered && (
                        <>
                          {" · "}
                          <span className={n < ordered ? "sup-down" : "sup-up"}>
                            {n < ordered
                              ? t("{count} short", { count: fmt.integer(ordered - n) })
                              : t("{count} extra", { count: fmt.integer(n - ordered) })}
                          </span>
                        </>
                      )}
                    </span>
                  </label>
                  <div className="sup-stepper">
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={t("One less")}
                      disabled={!n}
                      onClick={() => set(String(Math.max(0, (n ?? 0) - 1)))}
                    >
                      <Icon name="minus" size={18} />
                    </button>
                    <input
                      id={id}
                      className="input"
                      inputMode="numeric"
                      value={l.arrived}
                      aria-invalid={n === null || undefined}
                      onFocus={(e) => e.currentTarget.select()}
                      onChange={(e) => set(e.currentTarget.value.trim())}
                    />
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={t("One more")}
                      onClick={() => set(String((n ?? 0) + 1))}
                    >
                      <Icon name="plus" size={18} />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        {locField && (
          <div className="field">
            <label className="flabel" htmlFor="rcv-loc">
              {fieldTitle(locField)}
            </label>
            <FieldInput field={locField} id="rcv-loc" value={location} onChange={setLoc} />
          </div>
        )}
        <p className="hint">
          {mayValidate(move)
            ? t("The goods are in stock as soon as you receive them.")
            : t("A stock move is made per line; a stock manager validates them.")}
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

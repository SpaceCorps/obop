// What a manufacturing order needs: its components against the stock at its production location,
// marked when short, and, while it has none, one button that copies them from the bill of
// materials scaled to the order's quantity (`manufacturing_order_line.bulk_create`, the call an
// agent makes). Finishing the order moves the stock (the package's `finish` effects).
import { useEffect, useState } from "react";
import { fmt, t } from "@obop/ui";
import { aggregate, search, type Row } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData } from "../../data.tsx";
import { actionConfirm } from "../../labels.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Button, Icon } from "../../ui.tsx";
import { MOVE, settleMoves } from "../inventory/moves.ts";
import "../supply.css";

export const MO = "manufacturing_order";
const LINE = "manufacturing_order_line";
const BOM = "manufacturing_bom";
const BOM_LINE = "manufacturing_bom_line";

type Need = { component: string; needed: number; onHand: number | null };

/** Units of a component for `qty` units of product, when one run of `perRun` uses `uses`. */
export const scaled = (uses: number, qty: number, perRun: number): number =>
  Math.ceil((uses * qty) / Math.max(1, perRun));

/**
 * The components of order `row` summed per product, against the units on hand at its production
 * location (`onHand` is null without a location); null when the lines cannot be read.
 */
async function needsOf(token: string, withStock: boolean, row: Row): Promise<Need[] | null> {
  const location = String(row.location_id ?? "");
  const lines = await search(token, LINE, {
    filter: [{ field: "order_id", op: "eq", value: row.id }],
    limit: 500,
  });
  if ("error" in lines) return null;
  const by = new Map<string, number>();
  for (const l of lines.rows)
    by.set(String(l.component_id), (by.get(String(l.component_id)) ?? 0) + Number(l.quantity ?? 0));
  let stock = new Map<string, number>();
  if (location && by.size && withStock) {
    const a = await aggregate(token, "inventory_item", {
      filter: [
        { field: "location_id", op: "eq", value: location },
        { field: "product_id", op: "in", value: [...by.keys()] },
      ],
      group_by: [{ field: "product_id" }],
      measures: [{ op: "sum", field: "quantity" }],
    });
    if (!("error" in a))
      stock = new Map(
        a.groups.map((g) => [String(g.key.product_id), Number(g.measures["sum:quantity"] ?? 0)]),
      );
  }
  return [...by].map(([component, needed]) => ({
    component,
    needed,
    onHand: location ? (stock.get(component) ?? 0) : null,
  }));
}

/** The components there are fewer of at the production location than the order uses. */
const shortOf = (needs: readonly Need[]) =>
  needs.filter((n) => n.onHand !== null && n.onHand < n.needed);

export function Components({ row }: RecordExtProps) {
  const d = useData();
  const [needs, setNeeds] = useState<Need[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const location = String(row.location_id ?? "");
  const open = row.state !== "done" && row.state !== "cancelled";
  const lineMeta = d.models.get(LINE)?.model;
  const canFill = open && !!lineMeta?.can_create && !!row.bom_id;
  const withStock = d.models.has("inventory_item");

  useEffect(() => {
    let live = true;
    void needsOf(d.token, withStock, row).then((n) => {
      if (live && n) setNeeds(n);
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id, location, d.mutations, withStock]);

  async function fill() {
    setBusy(true);
    setError("");
    const bom = await search(d.token, BOM, {
      filter: [{ field: "id", op: "eq", value: row.bom_id }],
      limit: 1,
    });
    const parts = await search(d.token, BOM_LINE, {
      filter: [{ field: "bom_id", op: "eq", value: row.bom_id }],
      limit: 500,
    });
    if ("error" in bom || "error" in parts) {
      setBusy(false);
      setError(serverMessage("error" in bom ? bom.error : (parts as { error: string }).error));
      return;
    }
    const perRun = Number((bom.rows[0] as Row | undefined)?.quantity ?? 1);
    const qty = Number(row.quantity ?? 0);
    const rows = parts.rows.map((p) => ({
      order_id: row.id,
      component_id: p.component_id,
      quantity: scaled(Number(p.quantity ?? 0), qty, perRun),
    }));
    if (!rows.length) {
      setBusy(false);
      setError(t("The bill of materials has no components yet."));
      return;
    }
    const r = await d.run(`${LINE}.bulk_create`, { rows, mode: "all_or_nothing" });
    setBusy(false);
    if (r.kind === "error") setError(serverMessage(r.message));
    else {
      await d.changed(LINE);
      d.notify(t("{count} components added", { count: String(rows.length) }), "ok");
    }
  }

  if (needs === null) return null;
  const short = shortOf(needs);
  return (
    <section className="sup-avail" aria-label={t("Components")}>
      {needs.length === 0 ? (
        <div className="sup-avail-empty">
          <p className="hint">
            {location
              ? t("No components yet: finishing the order would add the product without using any.")
              : t(
                  "Add the components and a production location, and finishing the order moves the stock.",
                )}
          </p>
          {canFill && (
            <Button icon="plus" loading={busy} onClick={() => void fill()}>
              {t("Add components from the bill of materials")}
            </Button>
          )}
        </div>
      ) : !location ? (
        <p className="sup-note">
          <Icon name="info" size={16} />
          {t(
            "Choose a production location: finishing the order then takes the components from there and puts the product there.",
          )}
        </p>
      ) : short.length === 0 ? (
        <p className="sup-note sup-ok">
          <Icon name="check-circle" size={16} />
          {t("All {count} components are in stock at {location}.", {
            count: String(needs.length),
            location: d.refText("inventory_location", location),
          })}
        </p>
      ) : (
        <>
          <p className="sup-note sup-short">
            <Icon name="warning" size={16} />
            {t("{count} of {total} components are short at {location}.", {
              count: String(short.length),
              total: String(needs.length),
              location: d.refText("inventory_location", location),
            })}
          </p>
          <ul className="sup-shortlist">
            {short.map((n) => (
              <li key={n.component}>
                <span>{d.refText("product", n.component)}</span>
                <span className="hint">
                  {t("needs {needed}, has {have}", {
                    needed: fmt.integer(n.needed),
                    have: fmt.integer(n.onHand ?? 0),
                  })}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </section>
  );
}

/**
 * "Finish" of an order in progress: the package's confirm, then `manufacturing_order.finish`
 * (which makes the production moves), then the moves are validated when the person may, so the
 * stock is right when the order is done.
 */
export function FinishAction({ model, row, busy }: RecordExtProps) {
  const d = useData();
  const [running, setRunning] = useState(false);
  const finish = model.actions.find((a) => a.id === `${MO}.finish`);
  if (!finish || row.state !== "in_progress") return null;
  async function run() {
    // Short components make the stock at the production location go below zero: say which.
    setRunning(true);
    const short = shortOf((await needsOf(d.token, d.models.has("inventory_item"), row)) ?? []);
    setRunning(false);
    const at = d.refText("inventory_location", String(row.location_id ?? ""));
    const ok = await d.confirm(
      short.length === 1
        ? t(
            "{name} is short at {location}. Finishing leaves the stock there below zero. Finish anyway?",
            { name: d.refText("product", short[0]!.component), location: at },
          )
        : short.length
          ? t(
              "{count} components are short at {location}. Finishing leaves the stock there below zero. Finish anyway?",
              { count: String(short.length), location: at },
            )
          : (actionConfirm(finish!) ?? t("Mark production as finished?")),
      short.length ? t("Finish anyway") : t("Finish"),
      t("Cancel"),
    );
    if (!ok) return;
    setRunning(true);
    const r = await d.run(`${MO}.finish`, { id: row.id });
    if (r.kind !== "ok") {
      setRunning(false);
      d.notify(
        r.kind === "approval"
          ? t("Finishing was proposed and waits for approval.")
          : serverMessage(r.message),
        r.kind === "approval" ? "info" : "error",
      );
      return;
    }
    const { done, waiting } = await settleMoves(
      d.run,
      d.token,
      d.models.get(MOVE)?.model,
      String(row.number ?? ""),
    );
    await d.changed(MOVE);
    setRunning(false);
    // An order from before 0.3.0 may have no location or components: then nothing moved.
    d.notify(
      done + waiting === 0
        ? t("Production finished; no stock moved (the order has no location or components)")
        : waiting === 0
          ? t("Production finished; stock updated")
          : t("Production finished; {count} stock moves wait for a stock manager", {
              count: String(waiting),
            }),
      waiting === 0 && done > 0 ? "ok" : "info",
    );
  }
  return (
    <Button
      variant="primary"
      icon="check"
      loading={running}
      disabled={busy}
      onClick={() => void run()}
    >
      {t("Finish")}
    </Button>
  );
}

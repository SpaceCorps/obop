// "Receive goods", "Move stock" and "Scrap" from the Inventory home: one short form per kind of
// move, with the locations already chosen, that makes the move and validates it in one go
// (`inventory_move.create` then `inventory_move.validate`, the calls an agent makes). Moving or
// scrapping shows what is on hand where the goods leave, starts there when one place holds the
// most, and asks once more before the stock there would go below zero (allowed, as the package
// says, but rarely meant).
import { useEffect, useMemo, useState } from "react";
import { fmt, t, useWorded } from "@obop/ui";
import { search, type FieldMeta } from "../../api.ts";
import { useData, useRows } from "../../data.tsx";
import { fromEdit, toEdit, type Edit } from "../../edit.ts";
import { FieldInput } from "../../fields.tsx";
import { fieldTitle } from "../../labels.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Button, Dialog, Icon } from "../../ui.tsx";
import {
  firstLocation,
  ITEM,
  LOCATION,
  makeMove,
  mayValidate,
  MOVE,
  type MoveKind,
} from "./moves.ts";
import "../supply.css";

type Kind = Exclude<MoveKind, "adjustment">;

const COPY: Record<Kind, () => { title: string; button: string; lead: string }> = {
  receipt: () => ({
    title: t("Receive goods"),
    button: t("Receive"),
    lead: t("Goods that arrived from a supplier go into stock at the location you choose."),
  }),
  internal: () => ({
    title: t("Move stock"),
    button: t("Move"),
    lead: t("Stock leaves one location and arrives at the other."),
  }),
  scrap: () => ({
    title: t("Scrap"),
    button: t("Scrap"),
    lead: t("Damaged or lost goods leave stock for good."),
  }),
};

const ANYWAY: Record<Kind, () => string> = {
  receipt: () => t("Receive"),
  internal: () => t("Move anyway"),
  scrap: () => t("Scrap anyway"),
};

/** The fields each kind asks for, in order. */
const FIELDS: Record<Kind, string[]> = {
  receipt: ["product_id", "quantity", "to_location_id", "origin"],
  internal: ["product_id", "quantity", "from_location_id", "to_location_id"],
  // Scrap leaves the business, as a delivery does (no destination), so it is not counted as stock.
  scrap: ["product_id", "quantity", "from_location_id"],
};

/** Units of `product` per location, summed over its lots; locations of kind scrap are left out. */
function useStockOf(product: string): Map<string, number> | null {
  const d = useData();
  const [stock, setStock] = useState<{ product: string; at: Map<string, number> } | null>(null);
  useEffect(() => {
    if (!product) return;
    let live = true;
    void search(d.token, ITEM, {
      filter: [{ field: "product_id", op: "eq", value: product }],
      limit: 500,
    }).then((r) => {
      if (!live || "error" in r) return;
      const at = new Map<string, number>();
      for (const i of r.rows) {
        const loc = String(i.location_id ?? "");
        at.set(loc, (at.get(loc) ?? 0) + Number(i.quantity ?? 0));
      }
      setStock({ product, at });
    });
    return () => {
      live = false;
    };
  }, [d.token, product]);
  return product && stock?.product === product ? stock.at : null;
}

export function MoveDialog(p: { kind: Kind; onClose: () => void }) {
  const d = useData();
  const move = d.models.get(MOVE)?.model;
  const { rows: locations } = useRows(LOCATION);
  const copy = COPY[p.kind]();
  const fields = useMemo(
    () =>
      FIELDS[p.kind]
        .map((n) => move?.fields.find((f) => f.name === n))
        .filter((f): f is FieldMeta => !!f),
    [move, p.kind],
  );
  const [values, setValues] = useState<Record<string, Edit>>({});
  const productField = fields.find((f) => f.name === "product_id");
  const product = productField ? String(values.product_id ?? "") : "";
  const leaves = p.kind !== "receipt";
  const stock = useStockOf(leaves ? product : "");
  const scrapAt = new Set(locations.filter((l) => l.kind === "scrap").map((l) => l.id));
  // Where the product is: the most first, scrap left out.
  const held = stock
    ? [...stock].filter(([loc, n]) => n > 0 && !scrapAt.has(loc)).sort((a, b) => b[1] - a[1])
    : [];
  // The locations a kind starts with; the person may pick others. Goods leave from where most of
  // them are.
  const preset: Record<string, string> = {
    to_location_id: p.kind === "receipt" ? firstLocation(locations, "receiving") : "",
    from_location_id: held[0]?.[0] ?? "",
  };
  const value = (f: FieldMeta): Edit =>
    f.name in values ? values[f.name]! : toEdit(f, preset[f.name] || undefined);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [failed, setFailed] = useWorded();
  const [busy, setBusy] = useState(false);
  // The quantity the person was told is more than is there, and pressed again for.
  const [overOk, setOverOk] = useState("");
  const fromField = fields.find((f) => f.name === "from_location_id");
  const from = fromField ? String(value(fromField) ?? "") : "";
  const qtyField = fields.find((f) => f.name === "quantity");
  const qty = qtyField ? Number(value(qtyField)) : NaN;
  const there = leaves && stock && from ? (stock.get(from) ?? 0) : null;
  const over = there !== null && Number.isFinite(qty) && qty > 0 && qty > there;
  const overKey = `${product}|${from}|${qty}`;

  async function submit() {
    const body: Record<string, unknown> = { kind: p.kind };
    const errs: Record<string, string> = {};
    for (const f of fields) {
      const needed = f.name !== "origin";
      const r = fromEdit({ ...f, required: needed }, value(f));
      if ("error" in r) errs[f.name] = r.error;
      else if (r.value !== null && r.value !== "") body[f.name] = r.value;
    }
    if (typeof body.quantity === "number" && body.quantity <= 0)
      errs.quantity = t("Enter a quantity above zero");
    if (body.from_location_id && body.from_location_id === body.to_location_id)
      errs.to_location_id = t("Choose another location than where it leaves");
    setErrors(errs);
    if (Object.keys(errs).length) return;
    if (over && overOk !== overKey) {
      setOverOk(overKey);
      return;
    }
    setBusy(true);
    setFailed("");
    const out = await makeMove(d.run, move, body);
    setBusy(false);
    if (out.kind === "error") {
      setFailed(() => serverMessage(out.message));
      return;
    }
    const product = d.refText("product", String(body.product_id));
    const href = d.linkTo(MOVE, { id: out.id }) ?? undefined;
    if (out.kind === "done")
      d.notify(
        t("{product} × {quantity}: stock updated", {
          product,
          quantity: String(body.quantity),
        }),
        "ok",
        href ? { action: { label: t("Open"), href } } : undefined,
      );
    else
      d.notify(
        t("Move saved as a draft. {why}", { why: out.why }),
        "info",
        href ? { action: { label: t("Open"), href } } : undefined,
      );
    p.onClose();
  }

  return (
    <Dialog
      title={copy.title}
      size="sm"
      onClose={p.onClose}
      className="sup-dialog"
      footer={
        <>
          <Button onClick={p.onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={() => void submit()}>
            {over && overOk === overKey ? ANYWAY[p.kind]() : copy.button}
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
        <p className="hint">{copy.lead}</p>
        {fields.map((f, i) => {
          const id = `mv-${f.name}`;
          return (
            <div key={f.name} className="field">
              <label className="flabel" htmlFor={id}>
                {fieldTitle(f)}
                {f.name === "origin" && <span className="hint"> · {t("optional")}</span>}
              </label>
              <FieldInput
                field={f}
                id={id}
                autoFocus={i === 0}
                value={value(f)}
                invalid={!!errors[f.name]}
                describedBy={errors[f.name] ? `${id}-err` : undefined}
                onChange={(v) => setValues((x) => ({ ...x, [f.name]: v }))}
              />
              {errors[f.name] && (
                <p id={`${id}-err`} role="alert" className="error">
                  {errors[f.name]}
                </p>
              )}
              {f.name === "from_location_id" && leaves && product && stock && (
                <p className="hint sup-there">
                  {from
                    ? t("On hand here: {count}", { count: fmt.integer(there ?? 0) })
                    : held.length
                      ? t("In stock at {count} locations", { count: String(held.length) })
                      : t("Not in stock anywhere.")}
                </p>
              )}
            </div>
          );
        })}
        {over && (
          <p role="alert" className="sup-note sup-short">
            <Icon name="warning" size={16} />
            <span className="sup-receipts-text">
              {t(
                "Only {count} on hand at {location}. Going ahead leaves the stock there below zero.",
                {
                  count: fmt.integer(there ?? 0),
                  location: d.refText(LOCATION, from),
                },
              )}
            </span>
          </p>
        )}
        {!mayValidate(move) && (
          <p className="hint">{t("A stock manager validates it before the stock changes.")}</p>
        )}
        {failed && (
          <p role="alert" className="error">
            {failed}
          </p>
        )}
      </form>
    </Dialog>
  );
}

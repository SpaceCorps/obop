// A customer invoice as a printable page (`#/invoicing/print?id=…`): the brand, the customer, the
// lines and the total on a sheet of paper, and a Print button that opens the browser's print
// dialog (where "Save as PDF" makes a PDF). It only reads: `invoice.search`, `invoice_line.search`,
// `party.search` and `sale_order.search`, the calls an agent makes for the same data (OR-4).
import { useEffect, useState } from "react";
import { Button, Icon, InstanceLogo, fmt, getCurrency, pageBrand, t } from "@obop/ui";
import { search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import { optionLabel } from "../../format.ts";
import { href, type Go } from "../../route.ts";
import { invoiceTotals } from "./totals.ts";
import "./invoicing.css";

type Loaded =
  | { state: "loading" }
  | { state: "missing" }
  | { state: "error"; message: string }
  | { state: "ok"; invoice: Row; lines: Row[]; party?: Row; order?: Row };

async function first(token: string, model: string, id: unknown): Promise<Row | undefined> {
  if (typeof id !== "string" || !id) return undefined;
  const r = await search(token, model, { filter: [{ field: "id", op: "eq", value: id }] });
  return "error" in r ? undefined : r.rows[0];
}

export function InvoicePrint({ app, id, go }: { app: string; id: string; go: Go }) {
  const d = useData();
  const [data, setData] = useState<Loaded>({ state: "loading" });

  useEffect(() => {
    let live = true;
    void (async () => {
      const inv = await search(d.token, "invoice", {
        filter: [{ field: "id", op: "eq", value: id }],
      });
      if (!live) return;
      if ("error" in inv) return setData({ state: "error", message: inv.error });
      const invoice = inv.rows[0];
      if (!invoice) return setData({ state: "missing" });
      const [lines, party, order] = await Promise.all([
        search(d.token, "invoice_line", {
          filter: [{ field: "invoice_id", op: "eq", value: id }],
          limit: 1000,
        }),
        first(d.token, "party", invoice.party_id),
        first(d.token, "sale_order", invoice.sale_order_id),
      ]);
      if (!live) return;
      setData({
        state: "ok",
        invoice,
        lines: "error" in lines ? [] : lines.rows,
        party,
        order,
      });
    })();
    return () => {
      live = false;
    };
  }, [d.token, id]);

  const back = () => go({ app, model: "invoice", id, full: "1" });
  const invoiceModel = d.models.get("invoice")?.model;
  const stateField = invoiceModel?.fields.find((f) => f.name === "state");

  return (
    <section className="main panel inv-print" aria-label={t("Print invoice")}>
      <div className="content">
        <div className="inv-toolbar">
          <Button variant="ghost" icon="chevron-left" onClick={back}>
            {t("Back to the invoice")}
          </Button>
          <Button
            variant="primary"
            icon="printer"
            disabled={data.state !== "ok"}
            onClick={() => window.print()}
          >
            {t("Print or save as PDF")}
          </Button>
        </div>
        {data.state === "loading" ? (
          <div className="inv-sheet inv-wait" aria-busy="true" aria-label={t("Loading")} />
        ) : data.state === "missing" ? (
          <p className="hint">
            {t("This invoice does not exist or you may not read it.")}{" "}
            <a href={href({ app, model: "invoice" })}>{t("Back to the invoices")}</a>
          </p>
        ) : data.state === "error" ? (
          <p className="hint">{data.message}</p>
        ) : (
          <Sheet {...data} stateLabel={optionLabel(String(data.invoice.state ?? ""), stateField)} />
        )}
      </div>
    </section>
  );
}

function Sheet(p: { invoice: Row; lines: Row[]; party?: Row; order?: Row; stateLabel: string }) {
  const d = useData();
  const { invoice, lines } = p;
  const currency =
    typeof invoice.currency === "string" && /^[A-Z]{3}$/.test(invoice.currency)
      ? invoice.currency
      : getCurrency();
  const money = (minor: unknown) => fmt.money(Number(minor) || 0, currency);
  const totals = invoiceTotals(lines);
  const number = typeof invoice.number === "string" && invoice.number ? invoice.number : "";
  const state = String(invoice.state ?? "");
  const brand = pageBrand();
  const orderName = p.order ? d.recordLabel("sale_order", p.order) : "";
  return (
    <article className="inv-sheet" aria-label={number || t("Draft invoice")}>
      {state !== "posted" && (
        <p className="inv-stamp" role="note">
          <Icon name="warning" size={16} />
          {state === "cancelled"
            ? t("Cancelled: this is not a valid invoice.")
            : t("Draft: not a valid invoice until it is posted and numbered.")}
        </p>
      )}
      <header className="inv-head">
        <div className="inv-brand">
          <InstanceLogo variant="full" size={32} />
        </div>
        <div className="inv-title">
          <h1>{t("Invoice")}</h1>
          <p className="inv-number">{number || t("Draft")}</p>
        </div>
      </header>

      <section className="inv-parties">
        <div>
          <h2 className="inv-label">{t("Bill to")}</h2>
          <p className="inv-party-name">{String(p.party?.name ?? "—")}</p>
          {!!p.party?.email && <p>{String(p.party.email)}</p>}
          {!!p.party?.tax_id && <p>{t("VAT ID {id}", { id: String(p.party.tax_id) })}</p>}
        </div>
        <dl className="inv-facts">
          <div>
            <dt>{t("Invoice number")}</dt>
            <dd>{number || "—"}</dd>
          </div>
          {orderName && (
            <div>
              <dt>{t("Sales order")}</dt>
              <dd>{orderName}</dd>
            </div>
          )}
          <div>
            <dt>{t("Status")}</dt>
            <dd>{p.stateLabel}</dd>
          </div>
          <div>
            <dt>{t("Currency")}</dt>
            <dd>{currency}</dd>
          </div>
        </dl>
      </section>

      <table className="inv-lines">
        <thead>
          <tr>
            <th scope="col">{t("Description")}</th>
            <th scope="col" className="num">
              {t("Quantity")}
            </th>
            <th scope="col" className="num">
              {t("Unit price")}
            </th>
            <th scope="col" className="num">
              {t("Amount")}
            </th>
          </tr>
        </thead>
        <tbody>
          {lines.length === 0 ? (
            <tr>
              <td colSpan={4} className="inv-empty">
                {t("No lines yet.")}
              </td>
            </tr>
          ) : (
            lines.map((l) => (
              <tr key={l.id}>
                <td>
                  {String(
                    l.description ||
                      (l.product_id ? d.refText("product", String(l.product_id)) : "") ||
                      "—",
                  )}
                </td>
                <td className="num">{fmt.integer(Number(l.quantity) || 0)}</td>
                <td className="num">{money(l.unit_price_minor)}</td>
                <td className="num">{money(totals.amountOf(l))}</td>
              </tr>
            ))
          )}
        </tbody>
      </table>

      <dl className="inv-total">
        <div>
          <dt>{t("Total")}</dt>
          <dd>{money(invoice.total_minor ?? totals.total)}</dd>
        </div>
      </dl>

      <footer className="inv-foot">
        {brand?.name ? (
          <span>{t("Thank you for your business. {name}", { name: brand.name })}</span>
        ) : (
          <span>{t("Thank you for your business.")}</span>
        )}
      </footer>
    </article>
  );
}

// Sales additions to a quotation or order (UI plan 3.3): a Print button with a printable quotation,
// and on a confirmed order the invoice that confirming made, with a link to it. Both only read
// (`sale_order_line.search`, `invoice.search`); the CLI, REST and MCP read the same records.
import { useEffect, useState } from "react";
import { Button, Notice, t } from "@obop/ui";
import { call, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import { formatDate, isoDate, money, rowCurrency } from "../../format.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { PrintSheet } from "./PrintSheet.tsx";
import "./sales.css";

const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);
const str = (v: unknown) => (typeof v === "string" ? v : "");

/** Rows of `model` whose `field` is `id`, reloaded after every write of the session. */
function useChildren(model: string, field: string, id: string, enabled = true): Row[] | null {
  const d = useData();
  const [rows, setRows] = useState<Row[] | null>(null);
  const can = enabled && d.models.has(model);
  useEffect(() => {
    if (!can) return;
    let live = true;
    void call(d.token, `${model}.search`, {
      filter: [{ field, op: "eq", value: id }],
      limit: 500,
    }).then((r) => {
      if (live) setRows(r.kind === "ok" ? (r.value as { rows: Row[] }).rows : []);
    });
    return () => {
      live = false;
    };
  }, [d.token, model, field, id, can, d.mutations]);
  return can ? rows : [];
}

export function PrintQuote({ busy }: RecordExtProps) {
  return (
    <Button icon="printer" disabled={busy} onClick={() => window.print()}>
      {t("Print")}
    </Button>
  );
}

/** The quotation as it prints: hidden on screen, the only thing on paper (PrintSheet). */
export function QuoteDocument({ row }: RecordExtProps) {
  const d = useData();
  const lines = useChildren("sale_order_line", "order_id", row.id) ?? [];
  const cur = rowCurrency(row);
  const title =
    row.state === "confirmed"
      ? t("Order confirmation")
      : row.state === "cancelled"
        ? t("Cancelled order")
        : t("Quotation");
  const customer = str(row.party_id) ? d.refText("party", str(row.party_id)) : "";
  return (
    <PrintSheet>
      <article className="so-print">
        <header>
          <h1>{title}</h1>
          <p className="so-print-no">{str(row.number)}</p>
        </header>
        <dl className="so-print-meta">
          {customer && (
            <div>
              <dt>{t("Customer")}</dt>
              <dd>{customer}</dd>
            </div>
          )}
          <div>
            <dt>{t("Date")}</dt>
            <dd>{formatDate(isoDate(new Date()))}</dd>
          </div>
        </dl>
        <table>
          <thead>
            <tr>
              <th>{t("Description")}</th>
              <th className="num">{t("Quantity")}</th>
              <th className="num">{t("Unit price")}</th>
              <th className="num">{t("Amount")}</th>
            </tr>
          </thead>
          <tbody>
            {lines.map((l) => (
              <tr key={l.id}>
                <td>{str(l.description) || d.refText("product", str(l.product_id))}</td>
                <td className="num">{num(l.quantity)}</td>
                <td className="num">{money(num(l.unit_price_minor), cur)}</td>
                <td className="num">{money(num(l.amount_minor), cur)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th colSpan={3}>{t("Total")}</th>
              <td className="num">{money(num(row.total_minor), cur)}</td>
            </tr>
          </tfoot>
        </table>
      </article>
    </PrintSheet>
  );
}

/** A confirmed order: the invoice it made, ready for accounting, one tap away. */
export function OrderInvoice({ row }: RecordExtProps) {
  const d = useData();
  const confirmed = row.state === "confirmed";
  const invoices = useChildren("invoice", "sale_order_id", row.id, confirmed);
  if (!confirmed || !invoices?.length) return null;
  const inv = invoices[0]!;
  const to = d.linkTo("invoice", { id: inv.id });
  const draft = inv.state === "draft";
  return (
    <div className="so-invoice">
      <Notice
        tone="success"
        title={draft ? t("A draft invoice is ready") : t("Invoiced")}
        action={
          to ? (
            <Button size="sm" iconEnd="arrow-right" onClick={() => (location.hash = to)}>
              {t("Open invoice")}
            </Button>
          ) : undefined
        }
      >
        {draft
          ? t("Accounting posts it to send it to the customer.")
          : t("Invoice {number}", { number: str(inv.number) || "—" })}
      </Notice>
    </div>
  );
}

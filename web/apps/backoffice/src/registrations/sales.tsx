// Sales: Print on a quotation or order, the printable quotation, and on a confirmed order the
// invoice that confirming made (UI plan 3.3).
import { registerRecordExtension } from "../extensions.ts";
import { OrderInvoice, PrintQuote, QuoteDocument } from "../apps/sales/SaleOrderExtras.tsx";

registerRecordExtension("sale_order", {
  actions: [PrintQuote],
  panels: [OrderInvoice, QuoteDocument],
});

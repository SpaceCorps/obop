// Invoicing: a "Print" button on an invoice, which opens the invoice as a printable page
// (apps/invoicing/InvoicePrint.tsx, `#/invoicing/print?id=…`), and on a posted invoice its journal
// entry or a "Book to ledger" button (apps/invoicing/LedgerPanel.tsx).
import { lazy } from "react";
import { Button, t } from "@obop/ui";
import { LedgerPanel } from "../apps/invoicing/LedgerPanel.tsx";
import { registerAppPages, registerRecordExtension, type RecordExtProps } from "../extensions.ts";
import { href } from "../route.ts";

/** The route name of the printable invoice inside the Invoicing app. */
const PRINT = "print";

const InvoicePrint = lazy(() =>
  import("../apps/invoicing/InvoicePrint.tsx").then((m) => ({ default: m.InvoicePrint })),
);

function PrintButton({ row, busy }: RecordExtProps) {
  return (
    <Button
      icon="printer"
      disabled={busy}
      onClick={() => (location.hash = href({ app: "invoicing", model: PRINT, id: row.id }))}
    >
      {t("Print")}
    </Button>
  );
}

registerRecordExtension("invoice", { actions: [PrintButton], panels: [LedgerPanel] });

registerAppPages("invoicing", {
  use: (ctx) => {
    if (!ctx || ctx.route.model !== PRINT || !ctx.route.id) return {};
    return { page: <InvoicePrint app={ctx.app.id} id={ctx.route.id} go={ctx.go} /> };
  },
});

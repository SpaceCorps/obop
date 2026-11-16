// Accounting Sweden: a VAT report shows the return it prepares (apps/accounting_se/VatReturn.tsx):
// box 49 first, the boxes in the sections of the form, and "Compute from ledger" on a draft.
import { lazy, Suspense } from "react";
import { registerRecordExtension, type RecordExtProps } from "../extensions.ts";

const VatReturnPanel = lazy(() =>
  import("../apps/accounting_se/VatReturn.tsx").then((m) => ({ default: m.VatReturnPanel })),
);

function VatReturn(p: RecordExtProps) {
  return (
    <Suspense fallback={null}>
      <VatReturnPanel {...p} />
    </Suspense>
  );
}

registerRecordExtension("accounting_se_vat_report", { panels: [VatReturn] });

// Purchase: a confirmed order is received through "Receive goods" (the units that arrived per line,
// the location, and validation of the stock moves), and an order shows its receipts.
import { t } from "@obop/ui";
import { registerRecordExtension, type RecordExtProps } from "../extensions.ts";
import { ORDER, ReceiveAction } from "../apps/purchase/Receive.tsx";
import { SourceMoves } from "../apps/inventory/SourceMoves.tsx";

function Receipts({ row }: RecordExtProps) {
  if (row.state !== "received") return null;
  return (
    <SourceMoves
      origin={String(row.number ?? "")}
      doneText={(units) => t("{count} units received into stock", { count: units })}
      linkText={(count) => t("Receipts ({count})", { count })}
    />
  );
}

registerRecordExtension(ORDER, {
  actions: [ReceiveAction],
  panels: [Receipts],
  overrides: [`${ORDER}.receive`],
});

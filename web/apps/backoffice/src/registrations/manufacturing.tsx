// Manufacturing: an order shows whether its components are in stock at its production location,
// copies them from its bill of materials while it has none, and "Finish" validates the production
// moves it makes when the person may; a done order shows those moves.
import { t } from "@obop/ui";
import { registerRecordExtension, type RecordExtProps } from "../extensions.ts";
import { Components, FinishAction, MO } from "../apps/manufacturing/Components.tsx";
import { SourceMoves } from "../apps/inventory/SourceMoves.tsx";

function Production(p: RecordExtProps) {
  const { row } = p;
  if (row.state !== "done") return <Components {...p} />;
  return (
    <SourceMoves
      origin={String(row.number ?? "")}
      doneText={() => t("The components left stock and the product arrived.")}
      linkText={(count) => t("Stock moves ({count})", { count })}
    />
  );
}

registerRecordExtension(MO, {
  actions: [FinishAction],
  panels: [Production],
  overrides: [`${MO}.finish`],
});

// Reject beside Confirm on a trip in review (UI plan 3.6): a reviewer is told to "confirm or reject",
// so both are in sight, Confirm the filled one. The generic action row keeps a danger action in the
// More actions menu, so this button draws `mileage_trip.reject` itself (its generic entry is
// overridden): it asks for the reason the action needs and sends it with the action, in one call, as
// an agent sends it. The server decides who may reject (not the driver) and says why it refuses.
import { useState } from "react";
import { t } from "@obop/ui";
import type { Command, FieldMeta } from "../../api.ts";
import { ActionDialog } from "../../ActionDialog.tsx";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Button } from "../../ui.tsx";

export const REJECT = "mileage_trip.reject";

export function RejectAction({ model, row, busy }: RecordExtProps) {
  const d = useData();
  const [asking, setAsking] = useState(false);
  const [running, setRunning] = useState(false);
  const reject: Command | undefined = model.actions.find((a) => a.id === REJECT);
  if (!reject || row.state !== "in_review") return null;
  const needs = (reject.ui?.needs ?? [])
    .map((n) => model.fields.find((f) => f.name === n))
    .filter((f): f is FieldMeta => !!f);
  const label = t("Reject");

  async function run(values: Record<string, unknown>) {
    setAsking(false);
    setRunning(true);
    const r = await d.run(REJECT, { id: row.id, ...values });
    setRunning(false);
    if (r.kind === "ok") d.notify(reject!.ui?.done ?? t("Trip rejected"));
    else if (r.kind === "approval")
      d.notify(t("Rejecting was proposed and waits for approval."), "info");
    else d.notify(serverMessage(r.message), "error");
  }

  return (
    <>
      <Button
        variant="secondary"
        icon="close"
        loading={running}
        disabled={busy}
        onClick={() => setAsking(true)}
      >
        {label}
      </Button>
      {asking && (
        <ActionDialog
          action={reject}
          label={label}
          needs={needs}
          question={t("Reject this trip? The driver sees the reason and can correct the trip.")}
          danger
          currency={undefined}
          onCancel={() => setAsking(false)}
          onConfirm={(values) => void run(values)}
        />
      )}
    </>
  );
}

// Reject beside Confirm on a trip in review (UI plan 3.6): a reviewer is told to "confirm or reject",
// so both are in sight, Confirm the filled one. The generic action row keeps a danger action in the
// More actions menu, so this button draws `mileage_trip.reject` itself (its generic entry is
// overridden): it asks for the reason the action needs and sends it with the action, in one call, as
// an agent sends it. The server decides who may reject (not the driver) and says why it refuses.
import { useState } from "react";
import { t } from "@obop/ui";
import type { Command, FieldMeta } from "../../api.ts";
import type { Edit } from "../../edit.ts";
import { ActionDialog } from "../../ActionDialog.tsx";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { actionDone } from "../../labels.ts";
import { serverMessage } from "../../serverMessage.ts";
import { reviewRefusal } from "./track.ts";
import { Button } from "../../ui.tsx";

export const REJECT = "mileage_trip.reject";

export function RejectAction({ model, row, busy, value, saveFirst }: RecordExtProps) {
  const d = useData();
  const [asking, setAsking] = useState<Record<string, Edit> | null>(null);
  const [running, setRunning] = useState(false);
  const reject: Command | undefined = model.actions.find((a) => a.id === REJECT);
  if (!reject || row.state !== "in_review") return null;
  const needs = (reject.ui?.needs ?? [])
    .map((n) => model.fields.find((f) => f.name === n))
    .filter((f): f is FieldMeta => !!f);
  const label = t("Reject");
  // What the reviewer typed in the form (a reason) starts the dialog; the form is saved first, as
  // the action row saves before an action, so nothing typed is lost (R9).
  async function ask() {
    const typed = value ? Object.fromEntries(needs.map((f) => [f.name, value(f)])) : {};
    setRunning(true);
    const saved = saveFirst ? await saveFirst() : true;
    setRunning(false);
    if (saved) setAsking(typed);
  }

  async function run(values: Record<string, unknown>) {
    setAsking(null);
    setRunning(true);
    const r = await d.run(REJECT, { id: row.id, ...values });
    setRunning(false);
    if (r.kind === "ok") d.notify(actionDone(reject!) ?? t("Trip rejected"));
    else if (r.kind === "approval")
      d.notify(t("Rejecting was proposed and waits for approval."), "info");
    else {
      d.notify(reviewRefusal(r.message) ?? serverMessage(r.message), "error");
      // The trip may have moved on (reviewed by someone else): show it as it is now.
      await d.changed(model.name);
    }
  }

  return (
    <>
      <Button
        variant="secondary"
        icon="close"
        loading={running}
        disabled={busy}
        onClick={() => void ask()}
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
          initial={asking}
          onCancel={() => setAsking(null)}
          onConfirm={(values) => void run(values)}
        />
      )}
    </>
  );
}

// Submit on an ended trip, drawn here so that a private trip has none (owner answer of 2026-11-18,
// rule A): a private trip is never reviewed, the server refuses its submit, and the review panel
// says why in words (`TripReview`). Any other ended trip gets the button the generic action row
// would draw, which saves the form first and runs the command as an agent would run it.
import { useState } from "react";
import { t } from "@obop/ui";
import type { Command } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordExtProps } from "../../extensions.ts";
import { actionTitle } from "../../labels.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Button } from "../../ui.tsx";

export const SUBMIT = "mileage_trip.submit";

/** Whether the trip gets a Submit button: ended and not private. */
export function offersSubmit(row: { state?: unknown; purpose?: unknown }): boolean {
  return row.state === "ended" && row.purpose !== "private";
}

export function SubmitAction({ model, row, busy, saveFirst }: RecordExtProps) {
  const d = useData();
  const [running, setRunning] = useState(false);
  const submit: Command | undefined = model.actions.find((a) => a.id === SUBMIT);
  if (!submit || !offersSubmit(row)) return null;

  async function run() {
    setRunning(true);
    const saved = saveFirst ? await saveFirst() : true;
    if (!saved) {
      setRunning(false);
      return;
    }
    const r = await d.run(SUBMIT, { id: row.id });
    setRunning(false);
    if (r.kind === "ok") d.notify(submit!.ui?.done ?? t("Sent to review"));
    else if (r.kind === "approval")
      d.notify(t("Submitting was proposed and waits for approval."), "info");
    else {
      d.notify(serverMessage(r.message), "error");
      await d.changed(model.name);
    }
  }

  return (
    <Button variant="primary" loading={running} disabled={busy} onClick={() => void run()}>
      {actionTitle(submit)}
    </Button>
  );
}

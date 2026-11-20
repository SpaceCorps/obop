// The commands of a model as a whole (`whole_model` in /api/meta, docs/ISSUES.md U66): they take
// no record id, so the record panel leaves them out and the list offers them here, beside New.
// Queue a message and Send due messages of the Mail deliveries are the first.
import { useState } from "react";
import { getCurrency, t } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";
import { ActionDialog } from "./ActionDialog.tsx";
import { useData } from "./data.tsx";
import { actionDone, actionTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
import { Button } from "./ui.tsx";
import { questionOf } from "./workflow.ts";

/** The actions of `model` that are about the model as a whole. */
export function modelActions(model: ModelMeta): Command[] {
  return model.actions.filter((a) => a.whole_model === true);
}

export function ModelActions({ model }: { model: ModelMeta }) {
  const d = useData();
  const [asking, setAsking] = useState<Command | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const actions = modelActions(model);
  if (actions.length === 0) return null;

  const needsOf = (a: Command): FieldMeta[] =>
    (a.ui?.needs ?? [])
      .map((n) => model.fields.find((f) => f.name === n))
      .filter((f): f is FieldMeta => !!f);

  async function run(a: Command, input: Record<string, unknown>) {
    setAsking(null);
    setBusy(a.id);
    const r = await d.run(a.id, input);
    setBusy(null);
    const name = actionTitle(a);
    if (r.kind === "ok") {
      d.notify(actionDone(a) ?? t("{name} done", { name }));
      d.refreshAll();
    } else if (r.kind === "approval")
      d.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
    else d.notify(serverMessage(r.message), "error");
  }

  return (
    <>
      {actions.map((a) => (
        <Button
          key={a.id}
          size="sm"
          data-model-action={a.id}
          loading={busy === a.id}
          disabled={busy !== null}
          onClick={() =>
            needsOf(a).length > 0 || questionOf(model, a) ? setAsking(a) : void run(a, {})
          }
        >
          {actionTitle(a)}
        </Button>
      ))}
      {asking && (
        <ActionDialog
          action={asking}
          label={actionTitle(asking)}
          needs={needsOf(asking)}
          question={questionOf(model, asking)}
          danger={false}
          currency={getCurrency()}
          onCancel={() => setAsking(null)}
          onConfirm={(values) => void run(asking, values)}
        />
      )}
    </>
  );
}

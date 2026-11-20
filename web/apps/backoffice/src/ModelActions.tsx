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
  // Why the server refused the action asked in the dialog: it stays open with what was typed.
  const [refusal, setRefusal] = useState<string | null>(null);
  const actions = modelActions(model);
  if (actions.length === 0) return null;

  const needsOf = (a: Command): FieldMeta[] =>
    (a.ui?.needs ?? [])
      .map((n) => model.fields.find((f) => f.name === n))
      .filter((f): f is FieldMeta => !!f);

  const ask = (a: Command | null) => {
    setRefusal(null);
    setAsking(a);
  };

  /** Run `a`; from the dialog (`asked`) a refusal is shown in it and the dialog stays open. */
  async function run(a: Command, input: Record<string, unknown>, asked = false) {
    setRefusal(null);
    setBusy(a.id);
    const r = await d.run(a.id, input);
    setBusy(null);
    const name = actionTitle(a);
    if (r.kind !== "ok" && r.kind !== "approval") {
      // The server's words are kept and worded when drawn (D9), so a language switch rewords them.
      if (asked) setRefusal(r.message);
      else d.notify(serverMessage(r.message), "error");
      return;
    }
    setAsking(null);
    if (r.kind === "ok") {
      d.notify(actionDone(a) ?? t("{name} done", { name }));
      d.refreshAll();
    } else
      d.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
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
          onClick={() => (needsOf(a).length > 0 || questionOf(model, a) ? ask(a) : void run(a, {}))}
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
          busy={busy === asking.id}
          error={refusal === null ? null : serverMessage(refusal)}
          onCancel={() => ask(null)}
          onConfirm={(values) => void run(asking, values, true)}
        />
      )}
    </>
  );
}

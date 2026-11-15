// The question before an action runs (lane L5, plan 2.6 item 3): its `ui.confirm` text, or a
// confirm for a danger action, and the fields it declares in `ui.needs` (a reason to reject), which
// are sent with the action in the same call, as an agent sends them.
import { useState } from "react";
import { t } from "@obop/ui";
import type { Command, FieldMeta } from "./api.ts";
import { fromEdit, toEdit, type Edit } from "./edit.ts";
import { FieldInput } from "./fields.tsx";
import { fieldTitle } from "./labels.ts";
import { Button, Dialog } from "./ui.tsx";

export type ActionDialogProps = {
  action: Command;
  /** The button's words ("Reject", "Cancel order"). */
  label: string;
  /** The fields the action asks for. */
  needs: FieldMeta[];
  /** The sentence to confirm, when there is one. */
  question?: string;
  danger: boolean;
  currency: string | undefined;
  onCancel: () => void;
  /** The values typed, as the server takes them. */
  onConfirm: (values: Record<string, unknown>) => void;
};

export function ActionDialog(p: ActionDialogProps) {
  const [values, setValues] = useState<Record<string, Edit>>(() =>
    Object.fromEntries(p.needs.map((f) => [f.name, toEdit(f, undefined, p.currency)])),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});
  const submit = () => {
    const out: Record<string, unknown> = {};
    const errs: Record<string, string> = {};
    for (const f of p.needs) {
      // What an action asks for is needed, whatever the field says on its own.
      const r = fromEdit({ ...f, required: true }, values[f.name] ?? "", p.currency);
      if ("error" in r) errs[f.name] = r.error;
      else out[f.name] = r.value;
    }
    setErrors(errs);
    if (Object.keys(errs).length === 0) p.onConfirm(out);
  };
  return (
    <Dialog
      title={p.label}
      size="sm"
      role={p.needs.length ? "dialog" : "alertdialog"}
      onClose={p.onCancel}
      className="action-dialog"
      footer={
        <>
          <Button onClick={p.onCancel}>{t("Cancel")}</Button>
          <Button variant={p.danger ? "danger" : "primary"} onClick={submit}>
            {p.label}
          </Button>
        </>
      }
    >
      <form
        className="action-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        {p.question && <p className="confirm-msg">{p.question}</p>}
        {p.needs.map((f, i) => {
          const id = `act-${f.name}`;
          return (
            <div key={f.name} className="field">
              <label className="flabel" htmlFor={id}>
                {fieldTitle(f)}
              </label>
              <FieldInput
                field={f}
                id={id}
                autoFocus={i === 0}
                value={values[f.name] ?? ""}
                currency={p.currency}
                invalid={!!errors[f.name]}
                describedBy={errors[f.name] ? `${id}-err` : undefined}
                onChange={(v) => setValues({ ...values, [f.name]: v })}
              />
              {errors[f.name] && (
                <p id={`${id}-err`} role="alert" className="error">
                  {errors[f.name]}
                </p>
              )}
            </div>
          );
        })}
      </form>
    </Dialog>
  );
}

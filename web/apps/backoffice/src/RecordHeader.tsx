// The top of the record panel: the title bar, the status bar and the action row. Split from
// RecordPanel.tsx (F2); lane L5 owns it. The panel keeps the state and the commands; this file
// only draws them.
import type { ReactNode } from "react";
import { describeFor, inSentence, t } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import type { Edit } from "./edit.ts";
import type { RecordExtension } from "./extensions.ts";
import { optionLabel } from "./format.ts";
import { actionTitle, fieldTitle } from "./labels.ts";
import { Button, IconButton, Tooltip } from "./ui.tsx";
import { canMove, initialOption, notForRecord } from "./workflow.ts";

export type RecordHeaderProps = {
  model: ModelMeta;
  fields: FieldMeta[];
  /** null while a new record is being made. */
  row: Row | null;
  /** The model's name for one record ("Lead"). */
  title: string;
  /** The record's own name (its label field), for a saved record. */
  label: string;
  phone: boolean;
  busy: boolean;
  /** The field the status bar shows, when the model has one. */
  statusField: FieldMeta | undefined;
  /** The value of a field as the form holds it now (the draft, else the record). */
  value: (f: FieldMeta) => Edit;
  governed: (f: FieldMeta) => boolean;
  /** Pick a status of a new record (an ungoverned status only). */
  pickStatus: (s: string) => void;
  /** Move a saved record to a status. */
  setStatus: (s: string) => void;
  /** Run an action of the model on the saved record. */
  act: (id: string, needsApproval: boolean) => void;
  /** Whether the action row is drawn (a phone without actions keeps Delete in the form). */
  actionRow: boolean;
  /** The actions that have a button on this record. */
  shownActions: Command[];
  /** What the model's registrations add to the action row (extensions.ts). */
  extensions: Required<RecordExtension>;
  onDelete: () => void;
  onClose: () => void;
  /** Drawn between the status bar and the action row (the related records). */
  between?: ReactNode;
};

/** The title bar, the status bar, `between` and the action row of the record panel. */
export function RecordHeader(p: RecordHeaderProps) {
  const { model, fields, row, title, phone, busy, statusField, value, governed } = p;
  const creating = row === null;
  return (
    <>
      <header>
        {phone && (
          <IconButton
            icon="chevron-left"
            label={t("Back to list")}
            tooltip={false}
            className="back"
            onClick={p.onClose}
          />
        )}
        <div>
          <h2>{creating ? t("New {model}", { model: inSentence(title) }) : p.label}</h2>
          {!creating && <span className="hint">{title}</span>}
        </div>
        {!phone && <IconButton icon="close" label={t("Close record")} onClick={p.onClose} />}
      </header>
      {statusField && (
        <ol className="statusbar" aria-label={fieldTitle(statusField)}>
          {statusField.options?.map((s, idx, all) => {
            const on = String(value(statusField)) === s;
            const current = all.indexOf(String(value(statusField)));
            const gov = governed(statusField);
            const cls = on ? "on" : idx < current ? "done" : "";
            if (!gov) {
              const click = creating || model.can_update;
              return (
                <li key={s}>
                  <button
                    className={cls}
                    aria-current={on ? "step" : undefined}
                    disabled={!click || busy}
                    onClick={() => (creating ? p.pickStatus(s) : p.setStatus(s))}
                  >
                    {optionLabel(s, statusField)}
                  </button>
                </li>
              );
            }
            // Governed: a step is a move, enabled only when an action the person may run leads there.
            const mv = creating
              ? {
                  allowed: false,
                  current: on,
                  needsApproval: false,
                  reason: t("A new {noun} starts as {state}; it moves on through actions", {
                    noun: inSentence(title),
                    state: optionLabel(initialOption(statusField), statusField),
                  }),
                }
              : canMove(model, statusField, row[statusField.name] as string, s, title, row);
            const blocked = !mv.allowed;
            const text = `${optionLabel(s, statusField)}${mv.allowed && mv.needsApproval ? " *" : ""}`;
            const btn = (
              <button
                className={`${cls}${blocked && !on ? " blocked" : ""}`}
                aria-current={on ? "step" : undefined}
                aria-disabled={(blocked && !on) || busy || undefined}
                data-needs-approval={mv.needsApproval || undefined}
                title={
                  mv.allowed && mv.needsApproval
                    ? t("Needs a human's approval when an agent does it")
                    : undefined
                }
                onClick={() => {
                  if (mv.allowed && !busy) p.setStatus(s);
                }}
              >
                {text}
              </button>
            );
            return (
              <li key={s}>
                {mv.reason && !on ? (
                  <Tooltip
                    label={mv.reason}
                    side="top"
                    align={idx === 0 ? "start" : idx === all.length - 1 ? "end" : "center"}
                  >
                    {btn}
                  </Tooltip>
                ) : (
                  btn
                )}
              </li>
            );
          })}
        </ol>
      )}
      {p.between}
      {!creating && p.actionRow && (
        <div className="actionrow">
          {p.shownActions.map((a, ai) => {
            const f = a.field ? fields.find((x) => x.name === a.field) : undefined;
            const at = f ? String(row[f.name] ?? "") : "";
            const why =
              f && a.to && at === a.to
                ? t("Already {state}", { state: optionLabel(a.to, f) })
                : f && a.from && at !== a.from
                  ? t("Only available when {field} is {state}", {
                      field: fieldTitle(f),
                      state: optionLabel(a.from, f),
                    })
                  : notForRecord(model, a, row);
            const btn = (
              <Button
                key={a.id}
                title={
                  why
                    ? undefined
                    : `${describeFor(a)}${a.needs_approval ? ` ${t("(needs a human's approval when an agent runs it)")}` : ""}`
                }
                aria-disabled={why || busy ? true : undefined}
                onClick={() => {
                  if (!why && !busy) p.act(a.id, a.needs_approval);
                }}
              >
                {actionTitle(a)}
                {a.needs_approval ? " *" : ""}
              </Button>
            );
            return why ? (
              <Tooltip
                key={a.id}
                label={why}
                side="top"
                align={ai === 0 ? "start" : ai === p.shownActions.length - 1 ? "end" : "center"}
              >
                {btn}
              </Tooltip>
            ) : (
              btn
            );
          })}
          {p.extensions.actions.map((Ext, i) => (
            <Ext key={`ext-${i}`} model={model} fields={fields} row={row} busy={busy} />
          ))}
          {model.can_delete && !phone && (
            <Button
              variant="ghost"
              className="danger-text"
              icon="trash"
              disabled={busy}
              onClick={p.onDelete}
            >
              {t("Delete")}
            </Button>
          )}
        </div>
      )}
    </>
  );
}

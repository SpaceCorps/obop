// The record panel: the state of one record's form and the commands it runs. What it draws is in
// RecordHeader.tsx (title, status, actions), RelatedChips.tsx and FormBody.tsx (F2 of the UI plan);
// what an app adds to it is registered in extensions.ts.
import { useEffect, useRef, useState } from "react";
import { inSentence, t, useFocusTrap } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, toEdit, validate, type Edit } from "./edit.ts";
import { recordExtensionOf } from "./extensions.ts";
import { FormBody } from "./FormBody.tsx";
import { isMasked, isState, optionLabel, rowCurrency } from "./format.ts";
import { actionTitle, fieldTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
import { DeleteDialog } from "./DeleteDialog.tsx";
import { PHONE_QUERY, SHEET_QUERY, useMedia, useScrollLock } from "./media.ts";
import { RecordHeader } from "./RecordHeader.tsx";
import { RelatedChips } from "./RelatedChips.tsx";
import { nav } from "./route.ts";
import "./registrations.ts";
import { hiddenForRecord, initialOption, isGoverned, moveRecord } from "./workflow.ts";

type Props = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  defaults: Record<string, unknown>;
  withheld: string[];
  title: string;
  onClose: () => void;
  onSaved: (id: string) => void;
};

const without = <V,>(m: Record<string, V>, name: string): Record<string, V> =>
  Object.fromEntries(Object.entries(m).filter(([k]) => k !== name));

/** The record opens in a floating side panel, so the list stays in view (docs/spec/ux.md 4.1). */
export function RecordPanel({
  model,
  fields,
  row,
  defaults,
  withheld,
  title,
  onClose,
  onSaved,
}: Props) {
  const d = useData();
  const creating = row === null;
  const [draft, setDraft] = useState<Record<string, Edit>>({});
  // What went wrong is kept as names and as the server's own words, and the sentence is written
  // when it is drawn: text kept finished would stay in the language it was written in after a
  // change of language (the panel keeps its state through one).
  const [invalid, setInvalid] = useState<Record<string, true>>({}); // failed the check at Save
  const [refused, setRefused] = useState<Record<string, string>>({}); // the server's words, by field
  const [error, setError] = useState<{ raw: string } | null>(null); // the server's words, for the form
  const clearField = (name: string) => {
    setInvalid((m) => without(m, name));
    setRefused((m) => without(m, name));
  };
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const panel = useRef<HTMLElement>(null);
  // Up to 1024 px the record is a sheet over the list, full-screen on a phone (D2).
  const sheet = useMedia(SHEET_QUERY);
  const phone = useMedia(PHONE_QUERY);
  useFocusTrap(panel, sheet);
  useScrollLock(sheet);
  const statusField = fields.find(isState);
  const canEdit = creating ? model.can_create : model.can_update;
  // What the model's registrations add (extensions.ts): the Sign app's signing links.
  const ext = recordExtensionOf(model.name);
  // An action that is not for this record at all has no button (a Document request is not marked
  // signed by hand); `notForRecord` greys out the ones that are only wrong for now.
  const shownActions = row
    ? model.actions.filter((a) => !hiddenForRecord(model, a, row))
    : model.actions;
  // On a phone Delete sits at the end of the form instead, so it no longer holds the action row
  // open, except where a registration adds a button to the row (the signing link of a request).
  const actionRow =
    model.actions.length > 0 || (model.can_delete && (!phone || ext.actions.length > 0));

  // A governed field (D48) starts at its first option and then moves only through actions.
  const governed = (f: FieldMeta) => isGoverned(model, f);
  const currency = rowCurrency(row ?? (defaults as Record<string, unknown>));
  const baseline = (f: FieldMeta): Edit =>
    creating && governed(f)
      ? initialOption(f)
      : toEdit(f, row ? row[f.name] : defaults[f.name], currency);
  const value = (f: FieldMeta): Edit => (f.name in draft ? draft[f.name]! : baseline(f));
  const masked = (f: FieldMeta) => !!row && isMasked(row[f.name]) && !(f.name in draft);
  const changed = (f: FieldMeta) => f.name in draft && draft[f.name] !== baseline(f);
  const dirty = fields.some(changed);
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  // Leaving with unsaved edits asks first (hash navigation, tab close).
  useEffect(() => {
    nav.dirty = () => dirtyRef.current;
    const before = (e: BeforeUnloadEvent) => {
      if (dirtyRef.current) e.preventDefault();
    };
    addEventListener("beforeunload", before);
    return () => {
      if (nav.dirty) nav.dirty = null;
      removeEventListener("beforeunload", before);
    };
  }, []);

  // Focus the panel while open, and give focus back where it was.
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    panel.current?.focus({ preventScroll: true });
    return () => {
      if (opener && document.contains(opener)) opener.focus({ preventScroll: true });
    };
  }, []);

  const saveRef = useRef<() => void>(() => {});
  saveRef.current = () => void save();
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (dirtyRef.current || creating) saveRef.current();
      } else if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".overlay")) {
        onClose();
      }
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, [onClose, creating]);

  function failure(message: string) {
    const prefix = `${model.name}.`;
    const f = message.startsWith(prefix)
      ? fields.find((x) => message.startsWith(`${prefix}${x.name} `))
      : undefined;
    // The server's English is matched first, then shown in the person's language (when drawn).
    if (f) setRefused((e) => ({ ...e, [f.name]: message }));
    else setError({ raw: message });
  }

  /** Save or create; the id of the saved record, or null when nothing was saved. */
  async function save(): Promise<string | null> {
    setError(null);
    const editable = fields.filter((f) => !masked(f));
    const errs = validate(
      editable,
      value,
      creating ? undefined : (f) => changed(f) || f.required,
      currency,
      // A value left as stored is not held to limits lowered since it was written (D90).
      (f) => !creating && !changed(f),
    );
    setInvalid(Object.fromEntries(Object.keys(errs).map((name) => [name, true as const])));
    setRefused({});
    if (Object.keys(errs).length) {
      d.notify(t("Fix the highlighted fields first"), "error");
      return null;
    }
    const body: Record<string, unknown> = {};
    for (const f of editable) {
      const r = fromEdit(f, value(f), currency);
      if ("error" in r) continue;
      // Computed values and numbers are the server's: they are shown, never sent.
      if (f.computed || f.sequence) continue;
      if (creating) {
        if (governed(f)) continue; // the server starts it at the first option
        if (r.value !== null && (f.kind !== "bool" || r.value === true || f.name in draft))
          body[f.name] = r.value;
      } else if (changed(f)) body[f.name] = r.value;
    }
    setBusy(true);
    const r = creating
      ? await d.run(`${model.name}.create`, body)
      : await d.run(`${model.name}.update`, { id: row.id, ...body });
    setBusy(false);
    if (r.kind === "ok") {
      d.notify(creating ? t("{model} created", { model: title }) : t("Saved"));
      setDraft({});
      dirtyRef.current = false;
      const id = String((r.value as Row).id ?? row?.id);
      onSaved(id);
      return id;
    } else if (r.kind === "approval") {
      d.notify(
        t(
          "Approval needed: the change was proposed (plan {plan}). A human must approve it first.",
          {
            plan: r.planId.slice(0, 8),
          },
        ),
        "info",
      );
    } else failure(r.message);
    return null;
  }

  async function setStatus(s: string) {
    if (!statusField || creating || busy) return;
    if (!governed(statusField) && !model.can_update) return;
    setBusy(true);
    setError(null);
    const out = await moveRecord(d.run, model, statusField, row, s, title);
    setBusy(false);
    if (out.kind === "ok") {
      setDraft((x) => {
        const { [statusField.name]: _, ...rest } = x;
        return rest;
      });
      d.notify(
        t("{field}: {values}", {
          field: fieldTitle(statusField),
          values: optionLabel(s, statusField),
        }),
      );
    } else if (out.kind === "approval") d.notify(out.message, "info");
    else {
      // The server's own words (D48), also inline so they stay readable.
      setError({ raw: out.raw });
      d.notify(out.message, "error");
    }
  }

  async function act(id: string, needsApproval: boolean) {
    if (!row) return;
    setBusy(true);
    setError(null);
    const r = await d.run(id, { id: row.id });
    setBusy(false);
    const act = model.actions.find((a) => a.id === id);
    const name = act ? actionTitle(act) : actionTitle({ id });
    if (r.kind === "ok") d.notify(t("{name} done", { name }));
    else if (r.kind === "approval")
      d.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
    else {
      const message = serverMessage(r.message);
      setError({ raw: r.message });
      d.notify(
        needsApproval ? t("{name} was refused: {reason}", { name, reason: message }) : message,
        "error",
      );
    }
  }

  async function moveField(f: FieldMeta, to: string) {
    if (!row) return;
    setBusy(true);
    setError(null);
    const out = await moveRecord(d.run, model, f, row, to, title);
    setBusy(false);
    if (out.kind === "ok")
      d.notify(
        t("{field}: {values}", {
          field: fieldTitle(f),
          values: optionLabel(to, f),
        }),
      );
    else if (out.kind === "approval") d.notify(out.message, "info");
    else {
      setError({ raw: out.raw });
      d.notify(out.message, "error");
    }
  }

  return (
    <>
      {sheet && <div className="record-scrim" aria-hidden="true" onClick={onClose} />}
      <aside
        className={`record panel${sheet ? " sheet" : ""}${phone ? " phone" : ""}`}
        aria-label={t("{model} record", { model: title })}
        role={sheet ? "dialog" : undefined}
        aria-modal={sheet || undefined}
        ref={panel}
        tabIndex={-1}
      >
        <RecordHeader
          model={model}
          fields={fields}
          row={row}
          title={title}
          label={row ? d.recordLabel(model.name, row) : ""}
          phone={phone}
          busy={busy}
          statusField={statusField}
          value={value}
          governed={governed}
          pickStatus={(s) => {
            if (statusField) setDraft({ ...draft, [statusField.name]: s });
          }}
          setStatus={(s) => void setStatus(s)}
          act={(id, needsApproval) => void act(id, needsApproval)}
          actionRow={actionRow}
          shownActions={shownActions}
          extensions={ext}
          onDelete={() => setDeleting(true)}
          onClose={onClose}
          between={!creating && <RelatedChips model={model} id={row.id} />}
        />
        {!creating &&
          ext.panels.map((Panel, i) => (
            <Panel key={i} model={model} fields={fields} row={row} busy={busy} />
          ))}
        <FormBody
          model={model}
          fields={fields}
          row={row}
          title={title}
          canEdit={canEdit}
          phone={phone}
          busy={busy}
          dirty={dirty}
          statusField={statusField}
          currency={currency}
          withheld={withheld}
          error={error}
          invalid={invalid}
          refused={refused}
          value={value}
          masked={masked}
          changed={changed}
          governed={governed}
          onEdit={(name, v) => setDraft((x) => ({ ...x, [name]: v }))}
          clearField={clearField}
          onMove={(f, to) => void moveField(f, to)}
          onSubmit={() => void save()}
          onDiscard={() => {
            if (dirty) {
              setDraft({});
              setInvalid({});
              setRefused({});
              setError(null);
            } else onClose();
          }}
          onDelete={() => setDeleting(true)}
          save={save}
        />
        {deleting && row && (
          <DeleteDialog
            model={model}
            noun={inSentence(title)}
            records={[{ id: row.id, label: d.recordLabel(model.name, row) }]}
            onClose={() => setDeleting(false)}
            onDone={() => onClose()}
          />
        )}
      </aside>
    </>
  );
}

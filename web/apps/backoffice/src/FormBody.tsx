// The form of the record panel: every field, the sections an app registers (<SectionSlot>), the
// lines of a document (<LinesSlot>), the messages and the Save bar. Split from RecordPanel.tsx
// (F2); lane L5 owns this file, lane L7 owns what <LinesSlot> draws (LinesSection.tsx).
import { rich, t } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, type Edit } from "./edit.ts";
import { recordExtensionOf, type RecordSectionProps } from "./extensions.ts";
import { FieldInput, RefLabel } from "./fields.tsx";
import { optionLabel, shownLabel } from "./format.ts";
import { fieldTitle, fieldTitleByName } from "./labels.ts";
import { LinesSection } from "./LinesSection.tsx";
import { serverMessage } from "./serverMessage.ts";
import { Badge, Button, Icon, Kbd, Menu } from "./ui.tsx";
import { canMove, initialOption } from "./workflow.ts";

/** The sentence of a failed check, or nothing when the value is fine. */
const errorOf = (r: { value: unknown } | { error: string }): string | undefined =>
  "error" in r ? r.error : undefined;

/** What `<LinesSlot>` (and so LinesSection) gets. */
export type LinesSlotProps = {
  model: ModelMeta;
  /** The fields of the parent the person may read. */
  fields: FieldMeta[];
  /** The parent record; null while it is being made. */
  row: Row | null;
  creating: boolean;
  /** The person may write the parent (create for a new one, update otherwise). */
  canEdit: boolean;
  busy: boolean;
  /**
   * Save the parent form as Save does (a new parent is created first, as the first "Add a line"
   * needs); the id of the saved record, or null when it was not saved (a check failed, an approval
   * is pending, the server refused).
   */
  save: () => Promise<string | null>;
};

/** Where an app's sections go inside the form (extensions.ts `sections`); empty by default. */
export function SectionSlot(p: RecordSectionProps) {
  const { sections } = recordExtensionOf(p.model.name);
  return (
    <>
      {sections.map((S, i) => (
        <S key={i} {...p} />
      ))}
    </>
  );
}

/** Where the lines of a document go inside the form; LinesSection decides what (if anything). */
export function LinesSlot(p: LinesSlotProps) {
  return <LinesSection {...p} />;
}

export type FormBodyProps = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  /** The model's name for one record. */
  title: string;
  canEdit: boolean;
  phone: boolean;
  busy: boolean;
  dirty: boolean;
  statusField: FieldMeta | undefined;
  currency: string | undefined;
  withheld: string[];
  /** The server's words for the whole form. */
  error: { raw: string } | null;
  /** Fields that failed the check at Save, and the server's words by field. */
  invalid: Record<string, true>;
  refused: Record<string, string>;
  value: (f: FieldMeta) => Edit;
  masked: (f: FieldMeta) => boolean;
  changed: (f: FieldMeta) => boolean;
  governed: (f: FieldMeta) => boolean;
  /** A field was edited. */
  onEdit: (name: string, v: Edit) => void;
  /** The error of a field is cleared once it is edited again. */
  clearField: (name: string) => void;
  /** Move a governed field of the saved record. */
  onMove: (f: FieldMeta, to: string) => void;
  /** Save or create. */
  onSubmit: () => void;
  /** Discard the draft, or close when there is none. */
  onDiscard: () => void;
  onDelete: () => void;
  save: () => Promise<string | null>;
};

export function FormBody(p: FormBodyProps) {
  const d = useData();
  const { model, fields, row, title, canEdit, busy, statusField, currency, value } = p;
  const creating = row === null;
  return (
    <form
      className="fields"
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        if (p.dirty || creating) p.onSubmit();
      }}
    >
      {fields
        // A computed value or a number does not exist before the record is saved.
        .filter((f) => f !== statusField && !(creating && (f.computed || f.sequence)))
        .map((f) => {
          const isMasked_ = p.masked(f);
          const fid = `f-${model.name}-${f.name}`;
          const err = p.invalid[f.name]
            ? errorOf(fromEdit(f, value(f), currency, !creating && !p.changed(f)))
            : p.refused[f.name] !== undefined
              ? serverMessage(p.refused[f.name]!)
              : undefined;
          return (
            <div key={f.name} className={`field${f.kind === "bool" ? " inline" : ""}`}>
              <label className="flabel" htmlFor={fid}>
                {fieldTitle(f)}
                {f.required && (
                  <span className="req" title={t("Required")} aria-hidden="true">
                    *
                  </span>
                )}
                {f.personal_data && (
                  <span
                    className="shield"
                    title={t("Personal data ({class}). Agents see it masked.", {
                      class: f.class,
                    })}
                    role="img"
                    aria-label={t("Personal data, {class}", { class: f.class })}
                  >
                    <Icon name="shield" size={13} />
                  </span>
                )}
                {f.class === "financial" && <span className="chip">{t("financial")}</span>}
                {f.computed && (
                  <span className="chip" title={t("Calculated by the server; it cannot be edited")}>
                    {t("calculated")}
                  </span>
                )}
                {f.sequence && (
                  <span
                    className="chip"
                    title={t("A number the server assigns; it cannot be edited")}
                  >
                    {t("numbered")}
                  </span>
                )}
              </label>
              {isMasked_ ? (
                <div className="maskedfield">
                  <span className="masked" id={fid}>
                    {String(row?.[f.name])}
                  </span>
                  <span className="hint">{t("Masked for this session")}</span>
                  {canEdit && (
                    <button type="button" className="linkbtn" onClick={() => p.onEdit(f.name, "")}>
                      {t("Replace")}
                    </button>
                  )}
                </div>
              ) : p.governed(f) ? (
                <GovernedValue
                  model={model}
                  field={f}
                  row={row}
                  title={title}
                  id={fid}
                  busy={busy}
                  onMove={(to) => p.onMove(f, to)}
                />
              ) : (
                <FieldInput
                  field={f}
                  id={fid}
                  value={value(f)}
                  readOnly={!canEdit || !!f.computed || !!f.sequence}
                  shown={row ? shownLabel(f, row[f.name], row) : undefined}
                  altHint={(() => {
                    const alt = model.fields.find((x) => x.name === `${f.name}_alt`);
                    return alt
                      ? { title: fieldTitle(alt), empty: !String(value(alt)).trim() }
                      : undefined;
                  })()}
                  currency={currency}
                  invalid={!!err}
                  describedBy={err ? `${fid}-err` : undefined}
                  unchanged={!creating && !p.changed(f)}
                  onChange={(v) => {
                    p.onEdit(f.name, v);
                    if (err) p.clearField(f.name);
                  }}
                />
              )}
              {f.kind === "ref" &&
                !isMasked_ &&
                String(value(f)) &&
                d.linkTo(f.to ?? "", { id: String(value(f)) }) && (
                  <a
                    className="reflink small"
                    href={d.linkTo(f.to ?? "", { id: String(value(f)) })!}
                  >
                    {rich("Open {name}", {
                      name: <RefLabel field={f} id={String(value(f))} link={false} />,
                    })}
                  </a>
                )}
              {err && (
                <p id={`${fid}-err`} role="alert" className="error">
                  {err}
                </p>
              )}
            </div>
          );
        })}
      <SectionSlot
        model={model}
        fields={fields}
        row={row}
        creating={creating}
        canEdit={canEdit}
        busy={busy}
      />
      <LinesSlot
        model={model}
        fields={fields}
        row={row}
        creating={creating}
        canEdit={canEdit}
        busy={busy}
        save={p.save}
      />
      {p.withheld.length > 0 && (
        <p className="hint withheld">
          <Icon name="shield" size={13} />{" "}
          {t("Withheld from you: {fields}", {
            fields: p.withheld
              .map((w) => fieldTitleByName(w.split(".")[1] ?? w, fields))
              .join(", "),
          })}
        </p>
      )}
      {p.error && (
        <p role="alert" className="error banner">
          {serverMessage(p.error.raw)}
        </p>
      )}
      {/* On a phone Delete waits at the end of the form, away from the status pills. */}
      {!creating && p.phone && model.can_delete && (
        <Button
          variant="ghost"
          className="danger-text record-delete"
          icon="trash"
          disabled={busy}
          onClick={p.onDelete}
        >
          {t("Delete")}
        </Button>
      )}
      <footer className="formbar">
        <Button
          type="submit"
          variant="primary"
          loading={busy}
          disabled={(!p.dirty && !creating) || !canEdit}
        >
          {creating ? t("Create") : t("Save")}
        </Button>
        <Button onClick={p.onDiscard}>{p.dirty ? t("Discard") : t("Close")}</Button>
        {!canEdit && <span className="hint">{t("Read only for you")}</span>}
        <span className="grow" />
        <span className="hint kbd-hint">
          {rich("<kbd>⌘S</kbd> save <kbd>Esc</kbd> close", {}, { kbd: (c) => <Kbd>{c}</Kbd> })}
        </span>
      </footer>
    </form>
  );
}

/** A governed field on the form: its value as a badge, and the moves the person may make. */
function GovernedValue(p: {
  model: ModelMeta;
  field: FieldMeta;
  row: Row | null;
  title: string;
  id: string;
  busy: boolean;
  onMove: (to: string) => void;
}) {
  const { model, field: f, row } = p;
  const current = row ? String(row[f.name] ?? "") : initialOption(f);
  return (
    <div className="governed" id={p.id}>
      {current ? <Badge field={f} value={current} /> : <span className="dim">—</span>}
      {row ? (
        <Menu
          ariaLabel={t("Move {field}", { field: fieldTitle(f) })}
          triggerClassName="btn small"
          trigger={<>{t("Move to…")}</>}
          items={(f.options ?? [])
            .filter((o) => o !== current)
            .map((o) => {
              const mv = canMove(model, f, current, o, p.title, row ?? undefined);
              return {
                label: `${optionLabel(o, f)}${mv.needsApproval ? " *" : ""}`,
                disabled: p.busy || !mv.allowed,
                description: mv.reason || undefined,
                onSelect: () => p.onMove(o),
              };
            })}
        />
      ) : (
        <span className="hint">
          {t("Starts as {state}; it moves on through actions.", {
            state: optionLabel(initialOption(f), f),
          })}
        </span>
      )}
    </div>
  );
}

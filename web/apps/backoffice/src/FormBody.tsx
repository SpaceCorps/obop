// The form of the record panel (lane L5, plan 2.6 items 4 and 5): the fields in sections (two
// columns where the panel is wide), values the person cannot write as formatted text, the sections
// an app registers (<SectionSlot>), the lines of a document (<LinesSlot>), the messages and the
// Save bar. Lane L7 owns what <LinesSlot> draws (LinesSection.tsx).
import { useState } from "react";
import { rich, t } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import { fromEdit, type Edit } from "./edit.ts";
import { recordExtensionOf, type RecordSectionProps } from "./extensions.ts";
import { Cell, FieldFormProvider, FieldInput, formOrder, RefLabel } from "./fields.tsx";
import { optionLabel, shownLabel } from "./format.ts";
import { shortcut } from "./keys.ts";
import { fieldTitle, fieldTitleByName } from "./labels.ts";
import { LinesSection } from "./LinesSection.tsx";
import type { FormSection } from "./recordLayout.ts";
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
  /** The person may write this model (create for a new record, update otherwise). */
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
  /** The groups of fields to draw (recordLayout.ts `sectionsOf`). */
  sections: FormSection[];
  /** A field the person cannot write here: shown as text (server-set, locked, read only). */
  fixed: (f: FieldMeta) => boolean;
  /** Some field can be written: the Save bar is drawn. */
  editable: boolean;
  /** A field was edited. */
  onEdit: (name: string, v: Edit) => void;
  /** The error of a field is cleared once it is edited again. */
  clearField: (name: string) => void;
  /** Move a governed field of the saved record. */
  onMove: (f: FieldMeta, to: string) => void;
  /** Save or create. */
  onSubmit: () => void;
  /** Discard the draft (a new record with nothing typed closes). */
  onDiscard: () => void;
  save: () => Promise<string | null>;
};

/** Why a value is the server's, for the label's tooltip. */
function setBy(f: FieldMeta): string | undefined {
  if (f.computed) return t("Calculated by the server; it cannot be edited");
  if (f.sequence) return t("A number the server assigns; it cannot be edited");
  if (f.readonly) return t("Set by the server; it cannot be edited");
  return undefined;
}

export function FormBody(p: FormBodyProps) {
  const d = useData();
  const { model, fields, row, title, canEdit, busy, currency, value } = p;
  const creating = row === null;
  // The person's own choice of open and closed sections, by key; else the layout's.
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const wrong = (f: FieldMeta) => !!p.invalid[f.name] || p.refused[f.name] !== undefined;
  // A record the person may not change at all reads as a description list.
  const asList = !p.editable;

  const field = (f: FieldMeta) => {
    const isMasked_ = p.masked(f);
    const fid = `f-${model.name}-${f.name}`;
    const wide =
      f.kind === "longtext" || f.kind === "image" || f.kind === "file" || p.governed(f)
        ? " wide"
        : "";
    if (p.fixed(f) && !isMasked_ && !p.governed(f)) {
      const v = row ? row[f.name] : undefined;
      const Label = asList ? "dt" : "span";
      const Value = asList ? "dd" : "div";
      return (
        <div key={f.name} className={`field ro${wide}`}>
          <Label className="flabel" id={`${fid}-label`} title={setBy(f)}>
            {fieldTitle(f)}
          </Label>
          <Value
            id={fid}
            className={`fvalue${f.kind === "longtext" ? " long" : ""}`}
            aria-labelledby={asList ? undefined : `${fid}-label`}
          >
            <Cell field={f} value={v} currency={currency} row={row ?? undefined} />
          </Value>
        </div>
      );
    }
    const err = p.invalid[f.name]
      ? errorOf(fromEdit(f, value(f), currency, !creating && !p.changed(f)))
      : p.refused[f.name] !== undefined
        ? serverMessage(p.refused[f.name]!)
        : undefined;
    return (
      <div key={f.name} className={`field${f.kind === "bool" ? " inline" : ""}${wide}`}>
        <label className="flabel" htmlFor={fid} title={setBy(f)}>
          {fieldTitle(f)}
          {f.required && (
            <span className="req" title={t("Required")} aria-hidden="true">
              *
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
            readOnly={p.fixed(f)}
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
            <a className="reflink small" href={d.linkTo(f.to ?? "", { id: String(value(f)) })!}>
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
  };

  return (
    <form
      className={`fields${asList ? " readonly" : ""}`}
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        if (p.dirty || creating) p.onSubmit();
      }}
    >
      {/* The inputs see the rest of the record: fill_from, the slug, a document named after its file. */}
      <FieldFormProvider
        value={{
          model,
          peek: (name) => {
            const f = (fields.find((x) => x.name === name) ?? model.fields.find((x) => x.name === name));
            return f ? value(f) : undefined;
          },
          fill: (values) => {
            for (const [name, v] of Object.entries(values)) p.onEdit(name, v);
          },
        }}
      >
      {p.sections.map((s) => {
        const shown = s.fields.some(wrong) || (open[s.key] ?? !s.collapsed);
        const Grid = asList ? "dl" : "div";
        const body = (
          <>
            {s.personal && (
              <p className="hint fsec-note">
                <Icon name="shield" size={13} />
                {t("Contains personal data. Agents see it masked.")}
              </p>
            )}
            <Grid className="fgrid">{formOrder(s.fields).map(field)}</Grid>
          </>
        );
        if (!s.label)
          return (
            <section key={s.key} className="fsec plain">
              {body}
            </section>
          );
        const head = `fsec-${model.name}-${s.key}`;
        return (
          <section key={s.key} className="fsec" aria-labelledby={head}>
            <h3 className="fsec-head">
              <button
                type="button"
                id={head}
                aria-expanded={shown}
                onClick={() => setOpen({ ...open, [s.key]: !shown })}
              >
                <Icon name={shown ? "chevron-down" : "chevron-right"} size={16} />
                {s.label}
                {!shown && (
                  <span className="hint fsec-count">
                    {t("{count, plural, one {# field} other {# fields}}", {
                      count: s.fields.length,
                    })}
                  </span>
                )}
              </button>
            </h3>
            {shown && body}
          </section>
        );
      })}
      </FieldFormProvider>
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
        <p
          className="hint withheld"
          title={p.withheld.map((w) => fieldTitleByName(w.split(".")[1] ?? w, fields)).join(", ")}
        >
          <Icon name="shield" size={13} /> {t("Some private fields are hidden from you")}
        </p>
      )}
      {p.error && (
        <p role="alert" className="error banner">
          {serverMessage(p.error.raw)}
        </p>
      )}
      {p.editable ? (
        <footer className={`formbar${!p.dirty && !creating ? " clean" : ""}`}>
          <Button type="submit" variant="primary" loading={busy} disabled={!p.dirty && !creating}>
            {creating ? t("Create") : t("Save")}
          </Button>
          <Button onClick={p.onDiscard} disabled={!p.dirty && !creating}>
            {t("Discard")}
          </Button>
          <span className="grow" />
          <span className="hint kbd-hint">
            {rich(
              "<kbd>{key}</kbd> save <kbd>Esc</kbd> close",
              { key: shortcut("S") },
              { kbd: (c) => <Kbd>{c}</Kbd> },
            )}
          </span>
        </footer>
      ) : (
        !creating && (
          <p className="hint rec-readonly">
            <Icon name="lock" size={13} /> {t("Read only for you")}
          </p>
        )
      )}
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
                label: optionLabel(o, f),
                disabled: p.busy || !mv.allowed,
                description:
                  mv.reason ||
                  (mv.needsApproval ? t("An agent needs a person's approval for this") : undefined),
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

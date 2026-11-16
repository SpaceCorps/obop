// The top of the record panel (lane L5, plan 2.6 items 1 and 2): the title bar with the picture,
// the record's name, the trail and the pager; the summary with the key facts and figures; the
// status as a stepper; `between` (the related records); the lock banner; and the action row with
// one primary action, the other valid ones and a menu for the rest. The panel keeps the state and
// the commands; this file only draws them.
import type { ReactNode } from "react";
import { describeFor, fmt, inSentence, t } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import { BlobThumb } from "./BlobFields.tsx";
import type { Edit } from "./edit.ts";
import type { RecordExtension } from "./extensions.ts";
import { optionLabel } from "./format.ts";
import { fieldTitle } from "./labels.ts";
import type { Crumb } from "./lastRecord.ts";
import type { NextActions } from "./presentation.ts";
import { actionLabelFor, stepsOf, type Lock } from "./recordLayout.ts";
import { Avatar, Badge, Button, Icon, IconButton, Menu, Tooltip } from "./ui.tsx";
import { initialOption, notForRecord } from "./workflow.ts";

export type RecordHeaderProps = {
  model: ModelMeta;
  fields: FieldMeta[];
  /** null while a new record is being made. */
  row: Row | null;
  /** The model's name for one record ("Lead"). */
  title: string;
  /** The record's own name (`titleOf`), for a saved record. */
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
  /** Move a saved record to a status (an ungoverned status only; governed ones move by action). */
  setStatus: (s: string) => void;
  /** The actions of the saved record, sorted (presentation.ts `nextActions`). */
  actions: NextActions;
  /** Run an action: the panel saves first, asks what it needs and confirms what it must. */
  runAction: (a: Command) => void;
  /** What the model's registrations add to the action row (extensions.ts). */
  extensions: Required<RecordExtension>;
  /** Delete the record (behind the confirm of DeleteDialog); absent when it may not be deleted. */
  onDelete?: () => void;
  /** Open a new record with this one's values; absent when the page offers no copy. */
  onDuplicate?: () => void;
  onClose: () => void;
  /** Drawn between the status bar and the action row (the related records). */
  between?: ReactNode;
  /** The money the header shows on the right, with the record's currency. */
  figures: FieldMeta[];
  currency: string | undefined;
  /** The main ref and date not in the title, as words. */
  chips: string[];
  /** The record's picture field, when the model has one. */
  avatar?: FieldMeta;
  /** The records opened before this one, from where the person came. */
  crumbs: Crumb[];
  /** "3 / 27" in the list's order, with the neighbours; absent outside a list. */
  pager?: { index: number; total: number; prev?: () => void; next?: () => void };
  /** Whether the record fills the page, and how to switch; absent when it cannot. */
  full: boolean;
  onFull?: (on: boolean) => void;
  lock: Lock | null;
};

/** The shield that marks an action an agent may only propose (a person approves it). */
function ApprovalMark() {
  const text = t("An agent needs a person's approval for this");
  return (
    <span className="approval-mark" role="img" aria-label={text} title={text}>
      <Icon name="shield" size={13} />
    </span>
  );
}

/**
 * Where the public site shows a record: `ui.public_url` filled from the row ("/blog/{slug}"), once
 * the record is published (its status reads "published", or it has no status); null otherwise.
 */
function publicUrlOf(model: ModelMeta, row: Row): string | null {
  const tpl = model.ui?.public_url;
  if (!tpl) return null;
  const state = model.fields.find((f) => f.name === "state" && f.kind === "select");
  if (state && row.state !== "published") return null;
  let ok = true;
  const path = tpl.replace(/\{(\w+)\}/g, (_, n: string) => {
    const v = row[n];
    if (v === null || v === undefined || v === "") ok = false;
    return encodeURIComponent(String(v ?? ""));
  });
  if (!ok) return null;
  try {
    return new URL(path, location.origin).href;
  } catch {
    return null;
  }
}

/** The title bar, the summary, the status, `between`, the lock and the action row. */
export function RecordHeader(p: RecordHeaderProps) {
  const { model, fields, row, title, phone, busy, statusField, value, governed } = p;
  const creating = row === null;
  const noun = inSentence(title);
  const current = statusField ? String(value(statusField)) : "";
  const gov = statusField ? governed(statusField) : false;
  const why = (a: Command) => (row ? notForRecord(model, a, row) : "");

  const kicker =
    p.crumbs.length > 1 ? (
      <nav className="rec-crumbs" aria-label={t("Records you came from")}>
        <ol>
          {p.crumbs.slice(0, -1).map((c) => (
            <li key={c.key}>
              <a href={c.href}>{c.label}</a>
            </li>
          ))}
        </ol>
      </nav>
    ) : (
      !creating && <span className="hint rec-kind">{title}</span>
    );

  // A picture when there is one; initials only stand in for a person's photo (a page's cover
  // image that is not set is no reason for "SW").
  const personPhoto =
    !!p.avatar && /(^|_)(photo|avatar|portrait|headshot)(_|$)/.test(p.avatar.name);
  // A published page or post links to where the public site shows it (D95 `ui.public_url`).
  const siteUrl = row ? publicUrlOf(model, row) : null;
  const pic =
    p.avatar && row && (row[p.avatar.name] || personPhoto) ? (
      <span className="rec-avatar">
        {row[p.avatar.name] ? (
          <BlobThumb id={String(row[p.avatar.name])} alt={fieldTitle(p.avatar)} size={48} />
        ) : (
          <Avatar name={p.label} size={48} />
        )}
      </span>
    ) : null;

  const menu: Parameters<typeof Menu>[0]["items"] = [
    ...p.actions.danger.map((a) => ({
      label: actionLabelFor(a, noun),
      danger: true,
      disabled: busy || !!why(a),
      description: why(a) || undefined,
      onSelect: () => p.runAction(a),
    })),
    ...(p.actions.danger.length && (p.onDuplicate || p.onDelete) ? ["separator" as const] : []),
    ...(p.onDuplicate
      ? [{ label: t("Duplicate"), icon: "copy" as const, onSelect: p.onDuplicate }]
      : []),
    ...(p.onDelete
      ? [
          {
            label: t("Delete"),
            icon: "trash" as const,
            danger: true,
            disabled: busy,
            onSelect: p.onDelete,
          },
        ]
      : []),
  ];

  const shown = [...(p.actions.primary ? [p.actions.primary] : []), ...p.actions.secondary];
  const button = (a: Command, at: number) => {
    const no = why(a);
    const btn = (
      <Button
        key={a.id}
        variant={a === p.actions.primary ? "primary" : "secondary"}
        title={no ? undefined : describeFor(a)}
        aria-disabled={no || busy ? true : undefined}
        data-needs-approval={a.needs_approval || undefined}
        onClick={() => {
          if (!no && !busy) p.runAction(a);
        }}
      >
        {actionLabelFor(a, noun)}
        {a.needs_approval && <ApprovalMark />}
      </Button>
    );
    return no ? (
      <Tooltip
        key={a.id}
        label={no}
        side="top"
        align={at === 0 ? "start" : at === shown.length - 1 ? "end" : "center"}
      >
        {btn}
      </Tooltip>
    ) : (
      btn
    );
  };
  const actionRow = !creating && (shown.length > 0 || p.extensions.actions.length > 0);
  // The rest (danger actions, Duplicate, Delete) waits in one menu at the top, beside Close.
  const more =
    !creating && menu.length > 0 ? (
      <Menu
        ariaLabel={t("More actions")}
        triggerClassName="icon-btn rec-more"
        align="end"
        trigger={<Icon name="more" size={18} />}
        items={menu}
      />
    ) : null;
  const lastFigure = p.figures[p.figures.length - 1];

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
        {pic}
        <div className="rec-title">
          {kicker}
          <h2 title={creating ? undefined : p.label}>
            {creating ? t("New {model}", { model: noun }) : p.label}
          </h2>
          {siteUrl && (
            <a className="rec-site" href={siteUrl} target="_blank" rel="noopener">
              {t("View on site")}
              <Icon name="external" size={12} />
            </a>
          )}
        </div>
        {phone && more && <div className="rec-tools">{more}</div>}
        {!phone && (
          <div className="rec-tools">
            {p.pager && (
              <div className="rec-pager" role="group" aria-label={t("Records of the list")}>
                <IconButton
                  icon="chevron-up"
                  size="sm"
                  label={t("Previous record ({key})", { key: "Alt+↑" })}
                  disabled={!p.pager.prev}
                  onClick={p.pager.prev}
                />
                <span className="hint rec-pos">
                  {t("{index} / {total}", {
                    index: fmt.integer(p.pager.index + 1),
                    total: fmt.integer(p.pager.total),
                  })}
                </span>
                <IconButton
                  icon="chevron-down"
                  size="sm"
                  label={t("Next record ({key})", { key: "Alt+↓" })}
                  disabled={!p.pager.next}
                  onClick={p.pager.next}
                />
              </div>
            )}
            {more}
            {p.onFull && (
              <IconButton
                icon={p.full ? "panel-left" : "external"}
                label={p.full ? t("Show beside the list") : t("Open as page")}
                onClick={() => p.onFull?.(!p.full)}
              />
            )}
            <IconButton icon="close" label={t("Close record")} onClick={p.onClose} />
          </div>
        )}
      </header>
      {phone && row && (statusField || p.chips.length > 0 || lastFigure) && (
        <div className="rec-strip">
          {statusField && current && gov && <Badge field={statusField} value={current} />}
          {lastFigure && (
            <span className="amount">{fmt.money(row[lastFigure.name], p.currency)}</span>
          )}
          {p.chips.map((c) => (
            <span key={c} className="rec-chip">
              {c}
            </span>
          ))}
        </div>
      )}
      {!phone && row && (p.chips.length > 0 || p.figures.length > 0) && (
        <div className="rec-summary">
          {p.chips.length > 0 && (
            <ul className="rec-chips" aria-label={t("Key facts")}>
              {p.chips.map((c) => (
                <li key={c} className="rec-chip">
                  {c}
                </li>
              ))}
            </ul>
          )}
          {p.figures.length > 0 && (
            <dl className="rec-figures">
              {p.figures.map((f) => (
                <div key={f.name} className="rec-figure">
                  <dt>{fieldTitle(f)}</dt>
                  <dd className="amount">{fmt.money(row[f.name], p.currency)}</dd>
                </div>
              ))}
            </dl>
          )}
        </div>
      )}
      {statusField && !(phone && row && gov) && (
        <ol className={`statusbar${gov ? " stepper" : ""}`} aria-label={fieldTitle(statusField)}>
          {(gov ? stepsOf(model, statusField, current) : (statusField.options ?? [])).map(
            (s, idx, all) => {
              const on = current === s;
              const at = all.indexOf(current);
              const cls = on ? "on" : at >= 0 && idx < at ? "done" : "";
              if (gov)
                // A governed status moves only through actions (D48): the steps only show the way.
                return (
                  <li key={s} className={cls || undefined} aria-current={on ? "step" : undefined}>
                    <span className="step">
                      {cls === "done" && <Icon name="check" size={12} />}
                      {optionLabel(s, statusField)}
                    </span>
                  </li>
                );
              const click = creating || model.can_update;
              return (
                <li key={s}>
                  <button
                    type="button"
                    className={cls}
                    aria-current={on ? "step" : undefined}
                    disabled={!click || busy}
                    onClick={() => (creating ? p.pickStatus(s) : p.setStatus(s))}
                  >
                    {optionLabel(s, statusField)}
                  </button>
                </li>
              );
            },
          )}
        </ol>
      )}
      {creating && statusField && gov && (
        <p className="hint rec-starts">
          {t("A new {noun} starts as {state}; it moves on through actions", {
            noun,
            state: optionLabel(initialOption(statusField), statusField),
          })}
        </p>
      )}
      {p.between}
      {p.lock && (
        <p className="rec-lock" role="note">
          <Icon name="lock" size={15} />
          <span>{p.lock.message}</span>
        </p>
      )}
      {actionRow && (
        <div className="actionrow">
          {shown.map((a, i) => button(a, i))}
          {p.extensions.actions.map((Ext, i) => (
            <Ext key={`ext-${i}`} model={model} fields={fields} row={row} busy={busy} />
          ))}
        </div>
      )}
    </>
  );
}

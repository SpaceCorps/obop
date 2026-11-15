// How one value reads (Cell, RefLabel) and how it is typed (FieldInput), chosen by the field's kind
// and its display format (presentation.ts `displayOf`, the D95 `ui.display` hint). The inputs live
// in inputs/; this file picks one and wires it to the rest of the record (inputs/form.ts).
import { useId, type KeyboardEvent } from "react";
import { fmt, t } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { useData } from "./data.tsx";
import type { Edit } from "./edit.ts";
import {
  decimalText,
  formatDate,
  instantShort,
  integer,
  isEmpty,
  isMasked,
  money,
  optionLabel,
  shownLabel,
} from "./format.ts";
import { fieldTitle } from "./labels.ts";
import { counterState } from "./limits.ts";
import { BlobInput, BlobThumb, FileChip } from "./BlobFields.tsx";
import { epochToInstant } from "./instants.ts";
import { parseInline, plain } from "./markdown.ts";
import { displayOf, titleFieldsOf, titleOf } from "./presentation.ts";
import { Badge, Icon } from "./ui.tsx";
import type { CommonAttrs } from "./inputs/common.ts";
import { DateInput, DatetimeInput } from "./inputs/DateInput.tsx";
import {
  basisPointsText,
  clockText,
  durationText,
  kmText,
  linkOf,
  percentShare,
  titleFromFileName,
  urlSlug,
} from "./inputs/display.ts";
import { useFieldForm } from "./inputs/form.ts";
import { MarkdownField, MarkdownView } from "./inputs/MarkdownField.tsx";
import { CurrencySelect, MoneyInput } from "./inputs/MoneyInput.tsx";
import { RefPicker } from "./inputs/RefPicker.tsx";
import {
  BasisPointsInput,
  DurationInput,
  EpochInput,
  KmInput,
  PercentBar,
  PercentInput,
  TimeInput,
} from "./inputs/UnitInputs.tsx";
import "./inputs/inputs.css";

export { RefPicker } from "./inputs/RefPicker.tsx";
export { FieldFormProvider, useFieldForm, type FieldFormApi } from "./inputs/form.ts";

/** A value as it reads in a list, card or calendar chip. Principal-masked tokens keep their look. */
export function Cell({
  field,
  value,
  link = true,
  currency,
  row,
  relative = false,
}: {
  field: FieldMeta;
  value: unknown;
  link?: boolean;
  /** The record's currency when it has one of its own. */
  currency?: string;
  /** The record the value belongs to: a text field that shows an option label reads its condition there. */
  row?: Row;
  /** An instant close to now reads "in 3 days" (lists); the exact time stays in the title. */
  relative?: boolean;
}) {
  if (isEmpty(value)) return <span className="dim">—</span>;
  if (isMasked(value)) return <span className="masked">{String(value)}</span>;
  const display = displayOf(field);
  if (field.kind === "number" || field.kind === "decimal") {
    switch (display) {
      case "time_of_day":
        return <span className="amount">{clockText(value)}</span>;
      case "duration_minutes":
        return <span className="amount">{durationText(value)}</span>;
      case "percent_bp":
        return <span className="amount">{basisPointsText(value)}</span>;
      case "km_from_m":
        return <span className="amount">{kmText(value)}</span>;
      case "percent": {
        const share = percentShare(value);
        return (
          <span className="pctcell">
            {share !== null && <PercentBar share={share} />}
            <span className="amount">{fmt.percent(share ?? 0, 0)}</span>
          </span>
        );
      }
      case "epoch_seconds": {
        const s = instantShort(epochToInstant(Number(value)));
        return s ? (
          <time dateTime={epochToInstant(Number(value))} title={s.full}>
            {s.text}
          </time>
        ) : (
          <span>{String(value)}</span>
        );
      }
    }
  }
  switch (field.kind) {
    case "money":
      return <span className="amount">{money(value, currency)}</span>;
    case "number":
      return <span className="amount">{integer(value)}</span>;
    case "date":
      return <time dateTime={String(value)}>{formatDate(value)}</time>;
    case "datetime": {
      const s = instantShort(value);
      const when = relative && s ? relativeText(String(value)) : null;
      return (
        <time dateTime={String(value)} title={s ? s.full : `${String(value)} (UTC)`}>
          {when ?? s?.text ?? String(value)}
        </time>
      );
    }
    case "decimal":
      return <span className="amount">{decimalText(value, field)}</span>;
    case "bool":
      return value ? (
        <span className="yes">
          <Icon name="check" size={14} /> {t("Yes")}
        </span>
      ) : (
        <span className="dim">{t("No")}</span>
      );
    case "select":
      return <Badge field={field} value={String(value)} />;
    case "ref":
      return <RefLabel field={field} id={String(value)} link={link} />;
    case "image":
      return <BlobThumb id={String(value)} alt={fieldTitle(field)} size={40} />;
    case "file":
      return <FileChip id={String(value)} field={field} />;
    case "longtext": {
      const s = String(value);
      if (display === "markdown") return <>{markdownLine(s)}</>;
      if (display === "address")
        return (
          <>
            {s
              .split(/\s*\n\s*/)
              .filter(Boolean)
              .join(", ")}
          </>
        );
      return <>{shownLabel(field, value, row) ?? s}</>;
    }
    default: {
      const text =
        shownLabel(field, value, row) ??
        (typeof value === "object" ? JSON.stringify(value) : String(value));
      const href = link ? linkOf(display, value) : null;
      return href ? (
        <a
          className="valuelink"
          href={href}
          {...(display === "url" ? { target: "_blank", rel: "noopener noreferrer" } : {})}
          onClick={(e) => e.stopPropagation()}
        >
          {text}
        </a>
      ) : (
        <>{text}</>
      );
    }
  }
}

/** The first paragraph of Markdown as plain words, for a list or a card. */
function markdownLine(s: string): string {
  const line = s.split("\n").find((l) => l.trim() && !l.startsWith("```")) ?? "";
  const bare = line.replace(/^(#{1,6} |[-*] |\d+\. |> )/, "");
  const text = plain(parseInline(bare));
  return text.length > 120 ? `${text.slice(0, 119)}…` : text;
}

/** "in 3 days", "2 hours ago" for an instant within a month of now; null further away. */
export function relativeText(iso: string, now = Date.now()): string | null {
  const ms = Date.parse(iso.replace(/(\.\d{3})\d+Z$/, "$1Z"));
  if (!Number.isFinite(ms) || Math.abs(ms - now) > 30 * 86_400_000) return null;
  return fmt.relative(ms, now);
}

/** The label of the record a ref points at, a link when the person may open that model. */
export function RefLabel({
  field,
  id,
  link = true,
}: {
  field: FieldMeta;
  id: string;
  link?: boolean;
}) {
  const d = useData();
  const to = field.to ?? "";
  const text = useRefTitle(to, id);
  const target = link ? d.linkTo(to, { id }) : null;
  if (!target) return <span title={`${to} ${id}`}>{text}</span>;
  return (
    <a
      className="reflink"
      href={target}
      title={t("Open {name}", { name: text })}
      onClick={(e) => e.stopPropagation()}
    >
      {text}
    </a>
  );
}

/**
 * The name of a referenced record as a person knows it (presentation.ts `titleOf`: "1200 Bank",
 * never an id), from the cached rows; the short id only while they are not loaded.
 */
export function useRefTitle(to: string, id: string): string {
  const d = useData();
  const info = d.models.get(to);
  const row = info ? d.store.peek(to)?.find((r) => r.id === id) : undefined;
  if (!info || !row) return d.refText(to, id);
  return titleOf(info.model, row, {
    refText: d.refText,
    rowOf: (m, rid) => d.store.cached(m)?.find((r) => r.id === rid),
    modelOf: (m) => d.models.get(m)?.model,
    moduleId: info.module.id,
  });
}

/**
 * The order of the fields on a form: a picture first (it heads the record, like an avatar), then a
 * file with its preview, then the rest as declared.
 */
export function formOrder(fields: readonly FieldMeta[]): FieldMeta[] {
  const rank = (f: FieldMeta) => (f.kind === "image" ? 0 : f.kind === "file" ? 1 : 2);
  return [...fields].sort((a, b) => rank(a) - rank(b));
}

export type InputProps = {
  field: FieldMeta;
  value: Edit;
  onChange: (v: Edit) => void;
  readOnly?: boolean;
  id?: string;
  autoFocus?: boolean;
  invalid?: boolean;
  describedBy?: string;
  onKeyDown?: (e: KeyboardEvent) => void;
  /** A cell of a table (lines, inline edit): the control alone, tighter, no hints under it. */
  compact?: boolean;
  /** The currency of a money field: the record's own, else the instance's. */
  currency?: string;
  /** For a select: options that cannot be chosen now, with the reason (governed fields). */
  optionState?: (option: string) => { disabled: boolean; title?: string };
  /** For an image: the model's `<name>_alt` field, when it has one, and whether it is empty. */
  altHint?: { title: string; empty: boolean };
  /** For a read-only text or longtext: what to draw in place of the stored text (an option's label). */
  shown?: string;
  /** The value is the one already stored: a counter above the limit is then not an error (D90). */
  unchanged?: boolean;
};

/** The ids a control is described by: the error, and the counter while it shows. */
function describe(
  errorId: string | undefined,
  counterId: string,
  counter: unknown,
): string | undefined {
  return [errorId, counter ? counterId : ""].filter(Boolean).join(" ") || undefined;
}

/** `1 842 of 2 000 characters`: red once the text is longer than the field allows.
 *
 * The count itself is read with the field (`aria-describedby`) and changes with every key, so it
 * is not a live region. A screen reader is told in words when the text gets close to the limit and
 * when it goes over, once for each and never per key: the message is the same for every count in
 * a band. The region is always there for a field with a limit, so the first change is announced. */
function CharCounter({
  id,
  state,
  max,
}: {
  id: string;
  state: { used: number; max: number; over: boolean } | null;
  /** The limit of the field, when it has one. */
  max: number | undefined;
}) {
  if (max === undefined) return null;
  return (
    <>
      {state && (
        <small className={state.over ? "hint counter over" : "hint counter"} id={id}>
          {t("{used} of {max, plural, one {# character} other {# characters}}", {
            used: fmt.number(state.used),
            max: state.max,
          })}
        </small>
      )}
      <span className="sr-only" role="status">
        {!state
          ? ""
          : state.over
            ? t("Over the limit of {max, plural, one {# character} other {# characters}}", { max })
            : t("Close to the limit of {max, plural, one {# character} other {# characters}}", {
                max,
              })}
      </span>
    </>
  );
}

/** A link beside a value that is a phone number, an address, a web page or a place. */
function ValueLink({ display, value }: { display: string | undefined; value: Edit }) {
  const href = linkOf(display, value);
  if (!href) return null;
  const what =
    display === "tel"
      ? { icon: "external" as const, say: t("Call") }
      : display === "email"
        ? { icon: "mail" as const, say: t("Write an email") }
        : display === "address"
          ? { icon: "external" as const, say: t("Show on a map") }
          : { icon: "external" as const, say: t("Open the page") };
  return (
    <a
      className="icon-btn valuebtn"
      href={href}
      title={what.say}
      {...(display === "tel" || display === "email"
        ? {}
        : { target: "_blank", rel: "noopener noreferrer" })}
    >
      <Icon name={what.icon} size={14} />
      <span className="sr-only">{what.say}</span>
    </a>
  );
}

const INPUT_MODE: Record<string, "tel" | "email" | "url"> = {
  tel: "tel",
  email: "email",
  url: "url",
};

/** The control for one field, chosen by its kind and its display format. */
export function FieldInput(p: InputProps) {
  const { field: f, value, onChange, readOnly } = p;
  const form = useFieldForm();
  const readText = readOnly && p.shown !== undefined ? p.shown : String(value);
  // How full a text is (D90), shown only near the limit, and only where it can still be edited.
  const counterId = `${useId()}-count`;
  const editableText = !readOnly && (f.kind === "text" || f.kind === "longtext");
  const limited = editableText ? f.max_length : undefined;
  const counter = editableText ? counterState(f, String(value)) : null;
  const display = displayOf(f);
  const common: CommonAttrs = {
    id: p.id,
    autoFocus: p.autoFocus,
    // Over the limit of the field and about to be sent: invalid before the save says so. A value
    // left as stored is savable (D90 Decision 6), so it is not marked.
    "aria-invalid": p.invalid || (counter?.over && !p.unchanged) || undefined,
    "aria-describedby": p.describedBy,
    "aria-required": f.required || undefined,
    onKeyDown: p.onKeyDown,
  };
  const base = { field: f, value, onChange, readOnly, common, compact: p.compact };

  if (f.kind === "number" || f.kind === "decimal") {
    switch (display) {
      case "time_of_day":
        return <TimeInput {...base} />;
      case "duration_minutes":
        return <DurationInput {...base} />;
      case "percent":
        return <PercentInput {...base} />;
      case "percent_bp":
        return <BasisPointsInput {...base} />;
      case "km_from_m":
        return <KmInput {...base} />;
      case "epoch_seconds":
        return <EpochInput {...base} />;
    }
  }

  /** A text edit, plus the slug that follows the title until the person edits the slug. */
  const typeText = (next: string) => {
    onChange(next);
    if (!form || f.name === "slug") return;
    const slug = form.model.fields.find((x) => x.name === "slug" && x.kind === "text");
    const titleName =
      titleFieldsOf(form.model).find((n) => n !== "slug") ??
      form.model.fields.find((x) => x.name === "title" || x.name === "name")?.name;
    if (!slug || titleName !== f.name) return;
    const now = String(form.peek("slug") ?? "");
    if (!now || now === urlSlug(String(value))) form.fill({ slug: urlSlug(next) });
  };

  switch (f.kind) {
    case "bool":
      return (
        <label className="switch">
          <input
            {...common}
            type="checkbox"
            role="switch"
            checked={value === true}
            disabled={readOnly}
            onChange={(e) => onChange(e.target.checked)}
          />
          <span className="track" aria-hidden="true" />
          <span>{value === true ? t("Yes") : t("No")}</span>
        </label>
      );
    case "select":
      return (
        <select
          {...common}
          value={String(value)}
          disabled={readOnly}
          onChange={(e) => onChange(e.target.value)}
        >
          {(!f.required || value === "") && <option value="">—</option>}
          {f.options?.map((o) => {
            const st = p.optionState?.(o);
            return (
              <option key={o} value={o} disabled={st?.disabled} title={st?.title}>
                {optionLabel(o, f)}
              </option>
            );
          })}
        </select>
      );
    case "longtext":
      if (display === "markdown" && p.shown === undefined)
        return (
          <>
            <MarkdownField {...base} />
            {!p.compact && <CharCounter id={counterId} state={counter} max={limited} />}
          </>
        );
      return (
        <>
          <div className={display === "address" ? "withlink" : undefined}>
            <textarea
              {...common}
              aria-describedby={describe(p.describedBy, counterId, counter)}
              rows={p.compact ? 2 : display === "address" ? 3 : 4}
              value={readText}
              readOnly={readOnly}
              onChange={(e) => typeText(e.target.value)}
            />
            {display === "address" && !p.compact && <ValueLink display={display} value={value} />}
          </div>
          <CharCounter id={counterId} state={counter} max={limited} />
        </>
      );
    case "ref":
      return <RefPicker {...p} />;
    case "image":
    case "file":
      return (
        <BlobInput
          field={f}
          value={String(value)}
          onChange={onChange}
          readOnly={readOnly}
          id={p.id}
          invalid={p.invalid}
          describedBy={p.describedBy}
          altHint={p.altHint}
          compact={p.compact}
          onUploaded={(file) => {
            // A document is named after the file it was made from, while it has no name.
            if (!form || f.kind !== "file") return;
            const name = titleFieldsOf(form.model).find((n) =>
              form.model.fields.some((x) => x.name === n && x.kind === "text"),
            );
            if (name && !String(form.peek(name) ?? "").trim())
              form.fill({ [name]: titleFromFileName(file.name) });
          }}
        />
      );
    case "money":
      return <MoneyInput {...base} currency={p.currency} />;
    case "number":
      return (
        <input
          {...common}
          type="number"
          step={1}
          min={f.min}
          max={f.max}
          className="num"
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "date":
      return <DateInput {...base} />;
    case "decimal":
      return (
        <input
          {...common}
          type="text"
          inputMode="decimal"
          className="num"
          placeholder={fmt.number(0, {
            minimumFractionDigits: f.scale ?? 0,
            maximumFractionDigits: f.scale ?? 0,
          })}
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "datetime":
      return <DatetimeInput {...base} />;
    default: {
      if (f.name === "currency" && f.kind === "text" && p.shown === undefined)
        return <CurrencySelect {...base} />;
      const mode = display ? INPUT_MODE[display] : undefined;
      return (
        <>
          <div className={mode ? "withlink" : undefined}>
            <input
              {...common}
              aria-describedby={describe(p.describedBy, counterId, counter)}
              type="text"
              inputMode={mode}
              autoComplete={mode === "email" ? "email" : mode === "tel" ? "tel" : undefined}
              spellCheck={mode ? false : undefined}
              value={readText}
              readOnly={readOnly}
              onChange={(e) => typeText(e.target.value)}
            />
            {mode && !p.compact && <ValueLink display={display} value={value} />}
          </div>
          <CharCounter id={counterId} state={counter} max={limited} />
        </>
      );
    }
  }
}

export { MarkdownView };
export type { Row };

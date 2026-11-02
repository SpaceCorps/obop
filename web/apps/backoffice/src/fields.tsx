import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { fmt, getCurrency, inSentence, t } from "@obop/ui";
import { getTime, type FieldMeta, type Row } from "./api.ts";
import { useData, useRows } from "./data.tsx";
import type { Edit } from "./edit.ts";
import {
  decimalText,
  formatDate,
  instantText,
  integer,
  isEmpty,
  isMasked,
  money,
  optionLabel,
  shownLabel,
} from "./format.ts";
import { fieldTitle, modelTitle } from "./labels.ts";
import { counterState } from "./limits.ts";
import { BlobInput, BlobThumb, FileChip } from "./BlobFields.tsx";
import { Badge, Icon } from "./ui.tsx";

/** A value as it reads in a list, card or calendar chip. Principal-masked tokens keep their look. */
export function Cell({
  field,
  value,
  link = true,
  currency,
  row,
}: {
  field: FieldMeta;
  value: unknown;
  link?: boolean;
  /** The record's currency when it has one of its own. */
  currency?: string;
  /** The record the value belongs to: a text field that shows an option label reads its condition there. */
  row?: Row;
}) {
  if (isEmpty(value)) return <span className="dim">—</span>;
  if (isMasked(value)) return <span className="masked">{String(value)}</span>;
  switch (field.kind) {
    case "money":
      return <span className="amount">{money(value, currency)}</span>;
    case "number":
      return <span className="amount">{integer(value)}</span>;
    case "date":
      return <time dateTime={String(value)}>{formatDate(value)}</time>;
    case "datetime":
      return (
        <time dateTime={String(value)} title={`${String(value)} (UTC)`}>
          {instantText(value)}
        </time>
      );
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
    default:
      return (
        <>
          {shownLabel(field, value, row) ??
            (typeof value === "object" ? JSON.stringify(value) : String(value))}
        </>
      );
  }
}

/** "€" for EUR, "$" for USD, "zł" for PLN in the active format locale. */
function currencySymbol(currency: string): string {
  try {
    return (
      new Intl.NumberFormat(fmt.locale, {
        style: "currency",
        currency,
        currencyDisplay: "narrowSymbol",
      })
        .formatToParts(0)
        .find((p) => p.type === "currency")?.value ?? currency
    );
  } catch {
    return currency;
  }
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
  const text = d.refText(to, id);
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

type InputProps = {
  field: FieldMeta;
  value: Edit;
  onChange: (v: Edit) => void;
  readOnly?: boolean;
  id?: string;
  autoFocus?: boolean;
  invalid?: boolean;
  describedBy?: string;
  onKeyDown?: (e: KeyboardEvent) => void;
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

/** The control for one field, chosen by its kind. */
export function FieldInput(p: InputProps) {
  const { field: f, value, onChange, readOnly } = p;
  const readText = readOnly && p.shown !== undefined ? p.shown : String(value);
  // How full a text is (D90), shown only near the limit, and only where it can still be edited.
  const counterId = `${useId()}-count`;
  const editableText = !readOnly && (f.kind === "text" || f.kind === "longtext");
  const limited = editableText ? f.max_length : undefined;
  const counter = editableText ? counterState(f, String(value)) : null;
  const common = {
    id: p.id,
    autoFocus: p.autoFocus,
    // Over the limit of the field and about to be sent: invalid before the save says so. A value
    // left as stored is savable (D90 Decision 6), so it is not marked.
    "aria-invalid": p.invalid || (counter?.over && !p.unchanged) || undefined,
    "aria-describedby": p.describedBy,
    "aria-required": f.required || undefined,
    onKeyDown: p.onKeyDown,
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
      return (
        <>
          <textarea
            {...common}
            aria-describedby={describe(p.describedBy, counterId, counter)}
            rows={p.compact ? 2 : 4}
            value={readText}
            readOnly={readOnly}
            onChange={(e) => onChange(e.target.value)}
          />
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
        />
      );
    case "money": {
      const cur = p.currency ?? getCurrency();
      const read = String(value).trim() ? fmt.parseMoney(String(value), cur) : null;
      const hintId = `${p.id ?? "money"}-read`;
      return (
        <div className="moneyfield">
          <div className="moneyin">
            <input
              {...common}
              aria-describedby={
                [p.describedBy, read?.ambiguous ? hintId : ""].filter(Boolean).join(" ") ||
                undefined
              }
              type="text"
              inputMode="decimal"
              className="num"
              placeholder={fmt.moneyInput(0, cur)}
              value={String(value)}
              readOnly={readOnly}
              onChange={(e) => onChange(e.target.value)}
            />
            <span aria-hidden="true">{currencySymbol(cur)}</span>
          </div>
          {read?.ambiguous && (
            <small className="hint money-read" id={hintId} role="status">
              {t("Read as {amount}", { amount: fmt.money(read.minor, cur) })}
            </small>
          )}
        </div>
      );
    }
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
      return (
        <input
          {...common}
          type="date"
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      );
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
    case "datetime": {
      const zone = getTime().zone;
      const hintId = `${p.id ?? "datetime"}-zone`;
      return (
        <div className="datetimefield">
          <input
            {...common}
            aria-describedby={[p.describedBy, hintId].filter(Boolean).join(" ")}
            type="text"
            placeholder="2026-03-29 14:30"
            value={String(value)}
            readOnly={readOnly}
            onChange={(e) => onChange(e.target.value)}
          />
          <small className="hint" id={hintId}>
            {t("Time in {zone}; end with Z or +02:00 to say another zone", { zone })}
          </small>
        </div>
      );
    }
    default:
      return (
        <>
          <input
            {...common}
            aria-describedby={describe(p.describedBy, counterId, counter)}
            type="text"
            value={readText}
            readOnly={readOnly}
            onChange={(e) => onChange(e.target.value)}
          />
          <CharCounter id={counterId} state={counter} max={limited} />
        </>
      );
  }
}

/** Searchable picker over the target model's records (ARIA combobox). */
export function RefPicker({
  field,
  value,
  onChange,
  readOnly,
  id,
  autoFocus,
  invalid,
  describedBy,
  onKeyDown,
}: InputProps) {
  const d = useData();
  const to = field.to ?? "";
  const target = d.models.get(to);
  const targetTitle = target ? modelTitle(target.model, target.module.id) : to.replace(/_/g, " ");
  const { rows, loaded } = useRows(to);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const listId = useId();
  const box = useRef<HTMLDivElement>(null);
  const current = String(value);
  const shown = open ? query : current ? d.refText(to, current) : "";
  const options = useMemo(() => {
    const q = query.trim().toLowerCase();
    return rows
      .map((r) => ({ id: r.id, text: d.recordLabel(to, r) }))
      .filter((o) => !q || o.text.toLowerCase().includes(q))
      .slice(0, 50);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, query, to, d.lang]);
  useEffect(() => setActive(0), [query, open]);
  useEffect(() => {
    if (!open) return;
    const on = (e: MouseEvent) => {
      if (box.current && !e.composedPath().includes(box.current)) setOpen(false);
    };
    addEventListener("mousedown", on);
    return () => removeEventListener("mousedown", on);
  }, [open]);

  if (!d.models.has(to))
    return (
      <input
        id={id}
        value={current}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("{model} id", { model: to })}
      />
    );

  const pick = (rid: string) => {
    onChange(rid);
    setOpen(false);
    setQuery("");
  };
  const keys = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setOpen(true);
      setActive((a) => Math.min(a + 1, options.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && open) {
      e.preventDefault();
      e.stopPropagation();
      const o = options[active];
      if (o) pick(o.id);
    } else if (e.key === "Escape" && open) {
      e.stopPropagation();
      setOpen(false);
      setQuery("");
    } else if (e.key === "Tab") setOpen(false);
    else onKeyDown?.(e);
  };

  return (
    <div className="refpicker" ref={box}>
      <input
        id={id}
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={open && options[active] ? `${listId}-${active}` : undefined}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        aria-required={field.required || undefined}
        autoFocus={autoFocus}
        autoComplete="off"
        readOnly={readOnly}
        value={shown}
        placeholder={
          loaded ? t("Search {model}…", { model: inSentence(targetTitle) }) : t("Loading…")
        }
        onFocus={() => !readOnly && setOpen(true)}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onKeyDown={keys}
      />
      {!readOnly && current && (
        <button
          type="button"
          className="icon-btn clear"
          aria-label={t("Clear {field}", { field: fieldTitle(field) })}
          onClick={() => pick("")}
        >
          <Icon name="close" size={12} />
        </button>
      )}
      {open && (
        <ul
          id={listId}
          role="listbox"
          className="rp-list panel"
          aria-label={t("{field} options", { field: fieldTitle(field) })}
        >
          {!field.required && !query && (
            <li
              role="option"
              aria-selected={current === ""}
              className="dim"
              onMouseDown={() => pick("")}
            >
              {t("None")}
            </li>
          )}
          {options.map((o, i) => (
            <li
              key={o.id}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={i === active}
              onMouseEnter={() => setActive(i)}
              onMouseDown={(e) => {
                e.preventDefault();
                pick(o.id);
              }}
            >
              {o.text}
            </li>
          ))}
          {options.length === 0 && (
            <li className="dim">{loaded ? t("No match") : t("Loading…")}</li>
          )}
        </ul>
      )}
    </div>
  );
}

export type { Row };

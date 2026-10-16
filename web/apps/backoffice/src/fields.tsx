import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { fmt, getCurrency, inSentence, t } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { useData, useRows } from "./data.tsx";
import type { Edit } from "./edit.ts";
import { formatDate, integer, isEmpty, isMasked, money, optionLabel } from "./format.ts";
import { fieldTitle, modelTitle } from "./labels.ts";
import { Badge, Icon } from "./ui.tsx";

/** A value as it reads in a list, card or calendar chip. Principal-masked tokens keep their look. */
export function Cell({
  field,
  value,
  link = true,
  currency,
}: {
  field: FieldMeta;
  value: unknown;
  link?: boolean;
  /** The record's currency when it has one of its own. */
  currency?: string;
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
    default:
      return <>{typeof value === "object" ? JSON.stringify(value) : String(value)}</>;
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
};

/** The control for one field, chosen by its kind. */
export function FieldInput(p: InputProps) {
  const { field: f, value, onChange, readOnly } = p;
  const common = {
    id: p.id,
    autoFocus: p.autoFocus,
    "aria-invalid": p.invalid || undefined,
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
                {optionLabel(o)}
              </option>
            );
          })}
        </select>
      );
    case "longtext":
      return (
        <textarea
          {...common}
          rows={p.compact ? 2 : 4}
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "ref":
      return <RefPicker {...p} />;
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
    default:
      return (
        <input
          {...common}
          type="text"
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
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
  }, [rows, query, to]);
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

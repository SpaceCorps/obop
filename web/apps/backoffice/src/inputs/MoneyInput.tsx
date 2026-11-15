// Money and currencies (plan 2.7 item 4). An amount reads grouped ("12,450.00") and is typed as
// plain digits while the box has the focus; an optional amount has no "0.00" in it. A currency is
// chosen from the ISO codes, the instance's own first.
import { useId } from "react";
import { fmt, getCurrency, t } from "@obop/ui";
import { useTypedText, type BaseInputProps } from "./common.ts";

/** "€" for EUR, "$" for USD, "zł" for PLN in the active format locale. */
export function currencySymbol(currency: string): string {
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

/** The typed amount grouped for reading, or the text itself when it does not read as one. */
function grouped(text: string, currency: string): string {
  if (!text.trim()) return "";
  const r = fmt.parseMoney(text, currency);
  if (!r || r.ambiguous) return text;
  const exp = fmt.exponent(currency);
  return fmt.number(r.minor / 10 ** exp, {
    minimumFractionDigits: exp,
    maximumFractionDigits: exp,
  });
}

export function MoneyInput({
  field,
  value,
  onChange,
  readOnly,
  common,
  currency,
  compact,
}: BaseInputProps & { currency?: string }) {
  const cur = currency ?? getCurrency();
  const hintId = useId();
  const box = useTypedText(
    value,
    (v) => grouped(v, cur),
    (v) => v,
    (typed) => typed,
    onChange,
  );
  const read = box.focused && box.text.trim() ? fmt.parseMoney(box.text, cur) : null;
  return (
    <div className="moneyfield">
      <div className="moneyin">
        <input
          {...common}
          aria-describedby={
            [common["aria-describedby"], read?.ambiguous ? hintId : ""].filter(Boolean).join(" ") ||
            undefined
          }
          type="text"
          inputMode="decimal"
          className="num"
          placeholder={field.required && !readOnly ? fmt.moneyInput(0, cur) : ""}
          value={box.text}
          readOnly={readOnly}
          onFocus={box.onFocus}
          onBlur={box.onBlur}
          onChange={(e) => box.onChange(e.target.value)}
        />
        <span aria-hidden="true">{currencySymbol(cur)}</span>
      </div>
      {read?.ambiguous && !compact && (
        <small className="hint money-read" id={hintId} role="status">
          {t("Read as {amount}", { amount: fmt.money(read.minor, cur) })}
        </small>
      )}
    </div>
  );
}

/** The currencies offered first, after the instance's own. */
const COMMON = ["EUR", "USD", "GBP", "CHF", "SEK", "NOK", "DKK", "PLN", "CZK", "JPY", "CAD", "AUD"];

let allCodes: string[] | null = null;
/** Every ISO 4217 code this browser knows, else the common ones. */
export function currencyCodes(): string[] {
  if (allCodes) return allCodes;
  try {
    const intl = Intl as unknown as { supportedValuesOf?: (k: string) => string[] };
    allCodes = intl.supportedValuesOf?.("currency") ?? COMMON;
  } catch {
    allCodes = COMMON;
  }
  return allCodes;
}

/** "EUR · Euro" in the active language. */
export function currencyName(code: string): string {
  try {
    const n = new Intl.DisplayNames([fmt.locale], { type: "currency" }).of(code);
    return n && n !== code ? `${code} · ${n}` : code;
  } catch {
    return code;
  }
}

/** A currency code field: the instance's currency first, then the common ones, then the rest. */
export function CurrencySelect({ field, value, onChange, readOnly, common }: BaseInputProps) {
  const own = getCurrency();
  const v = String(value);
  const first = [...new Set([own, ...COMMON])];
  const rest = currencyCodes().filter((c) => !first.includes(c));
  const known = new Set([...first, ...rest]);
  return (
    <select
      {...common}
      className="currencyselect"
      value={v}
      disabled={readOnly}
      onChange={(e) => onChange(e.target.value)}
    >
      {(!field.required || v === "") && <option value="">—</option>}
      {v && !known.has(v) && <option value={v}>{v}</option>}
      <optgroup label={t("Common")}>
        {first.map((c) => (
          <option key={c} value={c}>
            {currencyName(c)}
          </option>
        ))}
      </optgroup>
      <optgroup label={t("All currencies")}>
        {rest.map((c) => (
          <option key={c} value={c}>
            {currencyName(c)}
          </option>
        ))}
      </optgroup>
    </select>
  );
}

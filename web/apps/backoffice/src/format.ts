import {
  fmt,
  getLang,
  labelFor,
  minorToDecimal,
  tx,
  t,
  validDate as validPlainDate,
} from "@obop/ui";
import { getTime, type FieldMeta } from "./api.ts";
import { formatDecimal } from "./decimals.ts";
import { formatInstant } from "./instants.ts";
import { shownOptionTarget } from "./shownOption.ts";

/**
 * The English, humanised form of a technical name: "expected_revenue_minor" -> "Expected revenue".
 * It is the message id of the built-in label tables (metaLabels.ts) and the fallback for any name
 * that has no translation. Use `fieldTitle` and friends from labels.ts to show a name.
 */
export function label(name: string): string {
  const s = name
    .replace(/_minor$/, "")
    .replace(/_id$/, "")
    .replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** "fleet_vehicle" in module "fleet" reads as "Vehicle" (English; labels.ts translates it). */
export function modelLabel(name: string, moduleId: string): string {
  return label(name.startsWith(`${moduleId}_`) ? name.slice(moduleId.length + 1) : name);
}

export const isStateName = (n: string) => n === "state" || n === "stage" || n === "status";
export const isState = (f: FieldMeta) => f.kind === "select" && isStateName(f.name);

/** English words for a select value, "in_service" reads as "In service". */
export function optionKey(v: string): string {
  const s = v.replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/**
 * The select value in the active language: the server's `option_labels` of `field` when it sent
 * one for the active language, else the client table of the built-in modules, else the English
 * words. Pass the field wherever it is known; without it only the client table applies.
 */
export function optionLabel(v: string, field?: Pick<FieldMeta, "option_labels">): string {
  if (v === "") return t("None");
  return labelFor({ labels: field?.option_labels?.[v] }, getLang(), tx("option", optionKey(v)));
}

/**
 * The label to draw instead of the stored text of a text field that shows an option (`shows_option_of`,
 * D52 amendment), or `undefined` when the value is drawn as stored. Only for what is read: an edit input,
 * a filter, the sort order, the CSV export and the API keep the stored text. `row` is the record the
 * value belongs to, which the field's condition reads.
 */
export function shownLabel(
  f: FieldMeta,
  v: unknown,
  row?: Record<string, unknown>,
): string | undefined {
  const target = shownOptionTarget(f, v, row);
  return target ? optionLabel(String(v), target) : undefined;
}

/** The currency of a record: its own `currency` field when it has a valid one, else the instance's. */
export function rowCurrency(row: Record<string, unknown> | undefined): string | undefined {
  const c = row?.currency;
  return typeof c === "string" && /^[A-Z]{3}$/.test(c) ? c : undefined;
}

/** Money is an integer in minor units, shown in the record's (or the instance's) currency. */
export function money(minor: unknown, currency?: string): string {
  return fmt.money(minor, currency);
}

export function integer(n: unknown): string {
  return fmt.integer(n);
}

/** "12,50" typed in a German locale becomes 1250; `null` when it is not a valid amount. */
export function parseMoney(s: string, currency?: string): number | null {
  return fmt.parseMoney(s, currency)?.minor ?? null;
}

/** The amount as typed in an edit field, in the locale's own decimal mark. */
export function majorString(minor: unknown, currency?: string): string {
  return fmt.moneyInput(minor, currency);
}

/**
 * A canonical decimal string ("12.50") as integer minor units (1250), by digits, never through
 * floating point; `null` when it has more decimals than the currency allows or is not a number.
 */
export function decimalToMinor(s: string, exponent: number): number | null {
  const m = /^(-?)(\d+)(?:\.(\d*))?$/.exec(s.trim());
  if (!m) return null;
  const frac = m[3] ?? "";
  if (frac.length > exponent && /[1-9]/.test(frac.slice(exponent))) return null;
  const n = Number(m[2]! + frac.slice(0, exponent).padEnd(exponent, "0"));
  return Number.isSafeInteger(n) ? (m[1] ? -n : n) : null;
}

/** The amount with a dot, always two or more decimals as the currency needs: for CSV and sorting. */
export function plainMajor(minor: unknown, currency?: string): string {
  const n = Number(minor);
  return Number.isFinite(n) ? minorToDecimal(Math.trunc(n), fmt.exponent(currency)) : "";
}

export const validDate = validPlainDate;

/** A decimal (D72) as it reads in the active locale, with its field's digits after the point. */
export function decimalText(v: unknown, f: Pick<FieldMeta, "scale">): string {
  return formatDecimal(v, fmt.locale, f.scale);
}

/** An instant (D72) as it reads on the wall clock of the instance's time zone. */
export function instantText(v: unknown): string {
  return formatInstant(v, getTime().zone, fmt.locale);
}

export function formatDate(s: unknown): string {
  return fmt.date(s);
}

export function monthLabel(key: string): string {
  return fmt.month(key) || t("None");
}

export const isoDate = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export const isEmpty = (v: unknown) => v === null || v === undefined || v === "";

/** A masked token the server issued to an agent looks like NAME_12. */
export function isMasked(v: unknown): boolean {
  return typeof v === "string" && /^[A-Z][A-Z0-9]*_\d+$/.test(v);
}

export function visibleFields(fields: FieldMeta[]): FieldMeta[] {
  return fields.filter((f) => f.class !== "secret");
}

/** Plain text of a value, for search, sorting and CSV. `refText` resolves a ref id to its label. */
export function plainText(
  f: FieldMeta,
  v: unknown,
  refText: (model: string, id: string) => string,
  currency?: string,
): string {
  if (isEmpty(v)) return "";
  if (isMasked(v)) return String(v);
  switch (f.kind) {
    case "money":
      return plainMajor(v, currency);
    case "bool":
      return v ? t("Yes") : t("No");
    // The exact text the server holds: a dot, no grouping, every digit; sortable and exportable.
    case "decimal":
    case "datetime":
      return String(v);
    case "date":
      return String(v);
    case "ref":
      return f.to ? refText(f.to, String(v)) : String(v);
    case "select":
      return optionLabel(String(v), f);
    default:
      return typeof v === "object" ? JSON.stringify(v) : String(v);
  }
}

/** Download text as a file; used by the CSV export. */
export function download(name: string, text: string, type = "text/csv;charset=utf-8") {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function csvCell(s: string): string {
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

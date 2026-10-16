import type { FieldMeta } from "./api.ts";

export function label(name: string): string {
  const s = name
    .replace(/_minor$/, "")
    .replace(/_id$/, "")
    .replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** "fleet_vehicle" in module "fleet" reads as "Vehicle". */
export function modelLabel(name: string, moduleId: string): string {
  return label(name.startsWith(`${moduleId}_`) ? name.slice(moduleId.length + 1) : name);
}

export const isStateName = (n: string) => n === "state" || n === "stage" || n === "status";
export const isState = (f: FieldMeta) => f.kind === "select" && isStateName(f.name);

/** Words used for select values, "in_service" reads as "In service". */
export function optionLabel(v: string): string {
  if (v === "") return "None";
  const s = v.replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

const eur = new Intl.NumberFormat(undefined, { style: "currency", currency: "EUR" });
const plain = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 });

/** Money is an integer in minor units, shown as EUR. */
export function money(minor: unknown): string {
  const n = Number(minor);
  return Number.isFinite(n) ? eur.format(n / 100) : "—";
}

export function integer(n: unknown): string {
  const v = Number(n);
  return Number.isFinite(v) ? plain.format(v) : "—";
}

/** "12.5" typed in major units becomes 1250; `null` when it is not a valid amount. */
export function parseMoney(s: string): number | null {
  const t = s.trim().replace(",", ".");
  if (!/^-?\d+(\.\d{1,2})?$/.test(t)) return null;
  return Math.round(parseFloat(t) * 100);
}

export function majorString(minor: unknown): string {
  const n = Number(minor);
  if (!Number.isFinite(n)) return "";
  return (n / 100).toFixed(2);
}

export function validDate(s: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(s)) return false;
  const [y, m, d] = s.split("-").map(Number) as [number, number, number];
  const dt = new Date(y, m - 1, d);
  return dt.getFullYear() === y && dt.getMonth() === m - 1 && dt.getDate() === d;
}

export function formatDate(s: unknown): string {
  if (typeof s !== "string" || !validDate(s)) return s ? String(s) : "—";
  const [y, m, d] = s.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function monthLabel(key: string): string {
  const [y, m] = key.split("-").map(Number);
  if (!y || !m) return "None";
  return new Date(y, m - 1, 1).toLocaleDateString(undefined, { year: "numeric", month: "long" });
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
): string {
  if (isEmpty(v)) return "";
  if (isMasked(v)) return String(v);
  switch (f.kind) {
    case "money":
      return majorString(v);
    case "bool":
      return v ? "Yes" : "No";
    case "date":
      return String(v);
    case "ref":
      return f.to ? refText(f.to, String(v)) : String(v);
    case "select":
      return optionLabel(String(v));
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

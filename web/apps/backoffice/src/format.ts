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

export const isMoney = (f: string) => f.endsWith("_minor");
export const isState = (f: string) => f === "state" || f === "stage" || f === "status";

export function money(minor: unknown): string {
  const n = Number(minor);
  if (!Number.isFinite(n)) return "—";
  return new Intl.NumberFormat(undefined, { style: "currency", currency: "EUR" }).format(n / 100);
}

export function display(field: string, v: unknown): string {
  if (v === null || v === undefined || v === "") return "—";
  if (isMoney(field)) return money(v);
  if (typeof v === "boolean") return v ? "Yes" : "No";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** A masked token the server issued to an agent looks like NAME_12. */
export function isMasked(v: unknown): boolean {
  return typeof v === "string" && /^[A-Z][A-Z0-9]*_\d+$/.test(v);
}

export function visibleFields(fields: FieldMeta[]): FieldMeta[] {
  return fields.filter((f) => f.class !== "secret");
}

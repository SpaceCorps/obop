/**
 * What a field input may know about the rest of the record it is part of: the other values and a
 * way to set them. The record form provides it (FormBody), and so can a line editor (L7); without
 * it every input works alone, as in the filter panel. It carries the client half of three rules
 * whose server half is D95 (PROPOSED):
 *
 * - `fill_from` ("product_id.list_price_minor"): picking a record fills the fields that follow it.
 *   Without the hint a line of a document follows its product: the description is the product's
 *   name and the unit price its list price (its cost on a purchase).
 * - `ref_filter` ("active = true && project_id = $project_id"): which records a picker offers.
 *   Without the hint a picker offers the records that share a ref of the same name that the form
 *   has already set (a task's milestone is one of the task's project).
 * - an upload names an untitled record, and a slug follows the title until it is edited.
 */
import { createContext, useContext } from "react";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import type { Edit } from "../edit.ts";
import { isEmpty } from "../format.ts";

export type FieldFormApi = {
  /** The model of the record being edited. */
  model: ModelMeta;
  /** The current value of another field of the record, as its input holds it. */
  peek: (name: string) => Edit | undefined;
  /** Set other fields as if the person had typed them (several at once, in one update). */
  fill: (values: Record<string, Edit>) => void;
};

const FieldFormCtx = createContext<FieldFormApi | null>(null);

/** Provide the record to the inputs inside (`<FieldFormProvider value={…}>`). */
export const FieldFormProvider = FieldFormCtx.Provider;

/** The record an input belongs to, or null when it stands alone. */
export function useFieldForm(): FieldFormApi | null {
  return useContext(FieldFormCtx);
}

// ---- fill_from --------------------------------------------------------------------------------

/** One fill: set `field` from the picked record's `source` (a field name of the target). */
export type Fill = { field: FieldMeta; source: string };

/**
 * The fields of `model` that picking a record in `ref` fills, with the field of the picked record
 * each one takes. The `fill_from` hints first; without any, the line-of-a-product rule.
 */
export function fillsFor(model: ModelMeta, ref: FieldMeta, target?: ModelMeta): Fill[] {
  const declared = model.fields
    .filter((f) => f.fill_from?.startsWith(`${ref.name}.`))
    .map((f) => ({ field: f, source: f.fill_from!.slice(ref.name.length + 1) }))
    .filter((x) => !x.source.includes("."));
  if (declared.length || model.fields.some((f) => f.fill_from) || !target) return declared;
  const has = (n: string) => target.fields.some((f) => f.name === n);
  if (!has("list_price_minor") || ref.to !== target.name) return [];
  // A line of a document: its product names it and prices it.
  if (!model.fields.some((f) => f.kind === "ref" && f.to !== ref.to && f.required)) return [];
  const out: Fill[] = [];
  const price = model.fields.find(
    (f) => f.kind === "money" && /^unit_price(_minor)?$/.test(f.name) && !f.computed,
  );
  const purchase = model.name.startsWith("purchase");
  if (price)
    out.push({
      field: price,
      source: purchase && has("cost_minor") ? "cost_minor" : "list_price_minor",
    });
  const desc = model.fields.find(
    (f) =>
      (f.kind === "text" || f.kind === "longtext") && /^(description|label|name)$/.test(f.name),
  );
  if (desc && has("name")) out.push({ field: desc, source: "name" });
  return out;
}

/**
 * The values to set when `ref` moves from the record `before` to `after`: a field is filled when it
 * is empty, or when it still holds what the earlier record gave it (the person did not change it).
 * `asEdit` turns a stored value into what the field's input holds.
 */
export function fillValues(
  fills: readonly Fill[],
  before: Row | undefined,
  after: Row | undefined,
  peek: (name: string) => Edit | undefined,
  asEdit: (f: FieldMeta, v: unknown) => Edit,
): Record<string, Edit> {
  const out: Record<string, Edit> = {};
  if (!after) return out;
  for (const { field, source } of fills) {
    if (field.computed || field.sequence || field.readonly) continue;
    const now = peek(field.name);
    const was = before ? asEdit(field, before[source]) : "";
    const empty = now === undefined || now === "" || now === false;
    if (!empty && String(now) !== String(was)) continue;
    const v = after[source];
    if (isEmpty(v)) continue;
    out[field.name] = asEdit(field, v);
  }
  return out;
}

// ---- ref_filter -------------------------------------------------------------------------------

/** One condition a picker applies: the target's `field` must (not) equal `value`. */
export type RefCond = { field: string; op: "eq" | "ne"; value: string | number | boolean };

const literal = (s: string, peek: (name: string) => Edit | undefined): unknown => {
  const v = s.trim();
  if (v === "true") return true;
  if (v === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(v)) return Number(v);
  const q = /^(["'])(.*)\1$/.exec(v);
  if (q) return q[2];
  if (v.startsWith("$")) return peek(v.slice(1));
  return undefined;
};

/**
 * The conditions of a `ref_filter` expression: `a = b`, `a == b` and `a != b` joined by `&&`, where
 * `b` is a literal or `$field` of the record being edited. A `$field` that is still empty drops its
 * condition (nothing to narrow by yet). What the client cannot read it leaves to the server.
 */
export function parseRefFilter(expr: string, peek: (name: string) => Edit | undefined): RefCond[] {
  const out: RefCond[] = [];
  for (const part of expr.split("&&")) {
    const m = /^\s*([a-z_][\w]*)\s*(==|=|!=)\s*(.+?)\s*$/.exec(part);
    if (!m) continue;
    const v = literal(m[3]!, peek);
    if (v === undefined || v === "") continue;
    if (typeof v !== "string" && typeof v !== "number" && typeof v !== "boolean") continue;
    out.push({ field: m[1]!, op: m[2] === "!=" ? "ne" : "eq", value: v });
  }
  return out;
}

/**
 * The picker's conditions for `ref` in `model`: its `ref_filter` hint, else the shared-scope rule.
 * The rule: a ref of the target with the same name and target as a ref of the form that is set
 * (`project_id` on a task and on a milestone) narrows the target to the same record.
 */
export function refConditions(
  model: ModelMeta | undefined,
  ref: FieldMeta,
  target: ModelMeta | undefined,
  peek: (name: string) => Edit | undefined,
): { conds: RefCond[]; scope: FieldMeta[] } {
  if (ref.ref_filter) return { conds: parseRefFilter(ref.ref_filter, peek), scope: [] };
  if (!model || !target || target.name === model.name) return { conds: [], scope: [] };
  const scope: FieldMeta[] = [];
  const conds: RefCond[] = [];
  for (const f of model.fields) {
    if (f.kind !== "ref" || f.name === ref.name || !f.to || f.to === target.name) continue;
    const there = target.fields.find((x) => x.name === f.name && x.kind === "ref" && x.to === f.to);
    const v = peek(f.name);
    if (!there || typeof v !== "string" || !v) continue;
    scope.push(f);
    conds.push({ field: f.name, op: "eq", value: v });
  }
  return { conds, scope };
}

/** Whether a row meets the conditions. */
export function meets(row: Row, conds: readonly RefCond[]): boolean {
  return conds.every((c) => {
    const v = row[c.field];
    const same =
      typeof c.value === "boolean" ? (v === true) === c.value : String(v ?? "") === String(c.value);
    return c.op === "eq" ? same : !same;
  });
}

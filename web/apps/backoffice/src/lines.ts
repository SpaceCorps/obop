// The rules of the lines section (LinesSection.tsx, lane L7 of the UI plan): which shape a child
// model takes inside its parent's form, its columns and how many fit, the values a new line starts
// with, what picking a product fills in, when the lines are frozen and the sums of the footer. Pure
// functions over `/api/meta` and rows, no React. Each reads the D95 hint first (`children`,
// `ui.list`, `fill_from`, `guard`) and falls back to a heuristic the tests pin.
import {
  fixedOnceCreated,
  type ChildMeta,
  type FieldMeta,
  type ModelGuard,
  type ModelMeta,
  type Row,
  type ServerOrder,
} from "./api.ts";
import { isEmpty } from "./format.ts";
import { isIdLike, stateFieldOf, summable } from "./presentation.ts";
import { initialOption, isGoverned } from "./workflow.ts";

export type LineShape = "lines" | "list" | "images" | "translations";

/** Data classes a line never shows in a column (they stay in the edit sheet, if readable). */
const PRIVATE = new Set(["secret", "confidential_hr", "special_category"]);
/** A number that orders the children (an image's place in the gallery). */
export const POSITION = /^(sequence|seq|position|sort_order)$/;

/**
 * How a child is drawn. A shape the server declares (`children[].shape`) wins. Otherwise images and
 * translations go by the name (presentation.ts `childrenOf`), a child with a workflow of its own (a
 * vehicle's services: a governed state, actions) is a compact list of records, and anything else
 * is a lines table.
 */
export function shapeOf(parent: ModelMeta, child: ChildMeta, model: ModelMeta): LineShape {
  if (parent.children && child.shape) return child.shape;
  if (child.shape === "images" && model.fields.some((f) => f.kind === "image")) return "images";
  if (child.shape === "translations") return "translations";
  const state = stateFieldOf(model);
  if (model.actions.length > 0 || (state && isGoverned(model, state))) return "list";
  return "lines";
}

/** Whether a line's field takes input: not computed, numbered, read-only, governed or the parent ref. */
export function editableField(model: ModelMeta, f: FieldMeta, via: string): boolean {
  return (
    f.name !== via &&
    !f.computed &&
    !f.sequence &&
    !f.readonly &&
    !(f.kind === "select" && isGoverned(model, f))
  );
}

/** Kinds that never become a column: they are edited in the sheet. */
const SHEET_ONLY = new Set(["longtext", "image", "file", "datetime"]);

/** Whether a field may be a column of the lines table at all. */
function columnable(f: FieldMeta, via: string): boolean {
  if (f.name === via || f.ui?.hidden) return false;
  if (SHEET_ONLY.has(f.kind) || PRIVATE.has(f.class)) return false;
  if (f.name === "currency" || POSITION.test(f.name) || isIdLike(f)) return false;
  return true;
}

/** The rank of a kind in the default column order. */
function kindRank(f: FieldMeta): number {
  if (f.computed) return 9;
  switch (f.kind) {
    case "ref":
      return f.required ? 0 : 1;
    case "text":
      return 2;
    case "number":
      return 3;
    case "decimal":
      return 4;
    case "money":
      return 5;
    case "date":
      return 6;
    case "select":
      return 7;
    case "bool":
      return 8;
    default:
      return 8;
  }
}

/**
 * The columns of a lines table, in order. `ui.list` decides when the child declares it. Otherwise:
 * refs (required first: the product, the account), text (the description), quantities, prices and
 * other amounts, dates, choices, switches, and computed values last (the line's amount). Long
 * text, files, images, date-times, identifiers, positions, the currency and the ref to the parent
 * are never columns; they are edited in the sheet.
 */
export function lineColumns(model: ModelMeta, via: string, fields = model.fields): FieldMeta[] {
  const byName = (n: string) => fields.find((f) => f.name === n);
  if (model.ui?.list?.length) {
    return model.ui.list
      .map(byName)
      .filter((f): f is FieldMeta => !!f && f.name !== via && !SHEET_ONLY.has(f.kind));
  }
  return fields
    .filter((f) => columnable(f, via))
    .map((f, i) => ({ f, i }))
    .sort((a, b) => kindRank(a.f) - kindRank(b.f) || a.i - b.i)
    .map((x) => x.f);
}

/** The narrowest a column can be and stay usable, in pixels. */
export function minWidth(f: FieldMeta): number {
  // An amount needs room for "31,050.00 €" (its value and its currency mark).
  if (f.computed) return f.kind === "money" || f.kind === "decimal" ? 108 : 72;
  switch (f.kind) {
    case "ref":
      return 128;
    case "text":
      return 128;
    case "number":
      return 72;
    case "decimal":
      return 84;
    case "money":
      return 124;
    case "date":
      return 128;
    case "select":
      return 104;
    case "bool":
      return 52;
    default:
      return 112;
  }
}

/** How much a column is worth keeping when the table is narrow; higher stays longer. */
function keepScore(f: FieldMeta, i: number, firstComputedMoney: FieldMeta | undefined): number {
  if (i === 0) return 100;
  if (f.required && !f.computed) return 90;
  if (f === firstComputedMoney) return 80;
  if (f.kind === "text") return 60;
  if (f.kind === "money" || f.kind === "decimal") return 55;
  if (f.kind === "number") return 50;
  if (f.kind === "ref") return 40;
  if (f.kind === "date") return 30;
  if (f.kind === "select") return 25;
  return 20;
}

/** The width of the column of row buttons (more, delete). */
export const ACTIONS_WIDTH = 68;

/**
 * Which columns fit `width` pixels: the lowest-worth columns go first (switches, dates, optional
 * refs) and are edited in the sheet instead. `fits` is false when even the first column, the
 * required ones and the line's amount do not fit: the section then draws cards.
 */
export function fitColumns(
  cols: FieldMeta[],
  width: number,
): { shown: FieldMeta[]; hidden: FieldMeta[]; fits: boolean } {
  const firstMoney = cols.find((f) => f.computed && (f.kind === "money" || f.kind === "decimal"));
  const scored = cols.map((f, i) => ({ f, i, s: keepScore(f, i, firstMoney) }));
  const budget = width - ACTIONS_WIDTH;
  const keep = new Set(cols);
  let used = cols.reduce((s, f) => s + minWidth(f), 0);
  const drop = [...scored].sort((a, b) => a.s - b.s || b.i - a.i);
  for (const x of drop) {
    if (used <= budget) break;
    if (x.s >= 80) break;
    keep.delete(x.f);
    used -= minWidth(x.f);
  }
  return {
    shown: cols.filter((f) => keep.has(f)),
    hidden: cols.filter((f) => !keep.has(f)),
    fits: used <= budget,
  };
}

/** The order children are listed in: their position number, else the order they were made in. */
export function lineOrder(model: ModelMeta): ServerOrder[] {
  const pos = model.fields.find((f) => f.kind === "number" && POSITION.test(f.name));
  // Ids are UUIDv7, so ascending ids are the order of creation.
  return pos
    ? [
        { field: pos.name, dir: "asc" },
        { field: "id", dir: "asc" },
      ]
    : [{ field: "id", dir: "asc" }];
}

/** The debit and credit fields of a journal entry's lines, when the child has both. */
export function balanceFields(model: ModelMeta): { debit: FieldMeta; credit: FieldMeta } | null {
  const debit = model.fields.find((f) => f.name === "debit" && f.kind === "money");
  const credit = model.fields.find((f) => f.name === "credit" && f.kind === "money");
  return debit && credit ? { debit, credit } : null;
}

/** A value as a number of minor units (money) or a plain number; 0 when empty or not a number. */
export function numeric(v: unknown): number {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string" && v.trim() !== "" && Number.isFinite(Number(v))) return Number(v);
  return 0;
}

/** The columns the footer adds up: summable money and decimals (quantities of mixed products are not). */
export function summedColumns(model: ModelMeta, cols: FieldMeta[]): FieldMeta[] {
  return cols.filter(
    (f) =>
      (f.kind === "money" || f.kind === "decimal") && summable(f, model) && !POSITION.test(f.name),
  );
}

/** The sum of a field over rows, in its stored unit (minor units for money). */
export function sumOf(rows: readonly Record<string, unknown>[], name: string): number {
  return rows.reduce((s, r) => s + numeric(r[name]), 0);
}

/** The debit minus the credit of the lines; 0 when they balance. */
export function differenceOf(
  model: ModelMeta,
  rows: readonly Record<string, unknown>[],
): number | null {
  const b = balanceFields(model);
  if (!b) return null;
  return sumOf(rows, b.debit.name) - sumOf(rows, b.credit.name);
}

/**
 * What a new line starts with, in stored values: the ref to its parent; the fields the parent has
 * under the same name and target (an entry's partner); the D95 `default` literals (presentation.ts
 * `newDefaults` reads the rest); a required quantity of 1; and on a journal entry the amount that
 * balances the lines so far (a credit when the debits are ahead, else a debit), the other side 0.
 */
export function lineDefaults(opts: {
  child: ModelMeta;
  via: string;
  parent: ModelMeta;
  parentRow: Row | null;
  parentId: string;
  siblings: readonly Record<string, unknown>[];
}): Record<string, unknown> {
  const { child, via, parent, parentRow, parentId, siblings } = opts;
  const out: Record<string, unknown> = { [via]: parentId };
  for (const f of child.fields) {
    if (!editableField(child, f, via)) continue;
    const same = parent.fields.find((p) => p.name === f.name && p.kind === f.kind);
    if (parentRow && same && f.kind === "ref" && same.to === f.to && !isEmpty(parentRow[f.name]))
      out[f.name] = parentRow[f.name];
    if (f.kind === "number" && f.required && /(^|_)(quantity|qty)$/.test(f.name)) out[f.name] = 1;
    if (f.kind === "select" && f.required && f.options?.length && !(f.name in out))
      out[f.name] = initialOption(f);
  }
  const b = balanceFields(child);
  if (b) {
    const diff = differenceOf(child, siblings) ?? 0;
    out[b.debit.name] = diff < 0 ? -diff : 0;
    out[b.credit.name] = diff > 0 ? diff : 0;
  }
  return out;
}

/** Names that say a text field describes the line. */
const DESCRIPTION = /^(description|label|name|title)$/;
/** Money fields of a line that a picked product prices. */
const UNIT_PRICE = /^(unit_price|price|list_price)(_minor)?$/;
/** Price fields of a product, the selling one first and the buying one first for purchases. */
const SELL = /^(list_price|sale_price|price)(_minor)?$/;
const BUY = /^(cost|cost_price|standard_price|purchase_price)(_minor)?$/;

/**
 * What picking a record in `ref` fills in on a line, as stored values: every field whose D95
 * `fill_from` starts with the ref (`product_id.list_price_minor`). Without any `fill_from` on the
 * child, a picked product (a target with a price) fills the description with its name and the
 * unit price with its price: the selling price, or the cost on a purchase. The caller keeps what
 * the person typed and replaces only what an earlier pick filled in.
 */
export function fillsFor(opts: {
  child: ModelMeta;
  ref: FieldMeta;
  target: ModelMeta | undefined;
  picked: Row | undefined;
  /** The picked record's label, for a description. */
  label: string;
}): Record<string, unknown> {
  const { child, ref, target, picked, label } = opts;
  const out: Record<string, unknown> = {};
  if (!picked) return out;
  const hinted = child.fields.filter((f) => f.fill_from);
  if (hinted.length) {
    for (const f of hinted) {
      const [head, ...rest] = f.fill_from!.split(".");
      if (head !== ref.name || rest.length !== 1) continue;
      const v = picked[rest[0]!];
      if (!isEmpty(v)) out[f.name] = v;
    }
    return out;
  }
  if (!target) return out;
  const prices = target.fields.filter((f) => f.kind === "money");
  const buying = /(^|_)purchase(_|$)/.test(child.name);
  const pick = (re: RegExp) => prices.find((f) => re.test(f.name) && !isEmpty(picked[f.name]));
  const price = buying ? (pick(BUY) ?? pick(SELL)) : (pick(SELL) ?? pick(BUY));
  if (!prices.some((f) => SELL.test(f.name) || BUY.test(f.name))) return out;
  for (const f of child.fields) {
    if (f.name === ref.name || f.computed || f.sequence || f.readonly) continue;
    if (f.kind === "text" && DESCRIPTION.test(f.name) && label) out[f.name] = label;
    if (f.kind === "money" && UNIT_PRICE.test(f.name) && price) out[f.name] = picked[price.name];
  }
  return out;
}

/** Whether a guard freezes a record: its `immutable_when`, or `append_only`. */
function guardFreezes(g: ModelGuard | undefined, row: Row | null): boolean {
  if (!g || !row) return false;
  if (g.append_only || fixedOnceCreated(g)) return true;
  const w = g.immutable_when;
  return !!w && w.values.includes(String(row[w.field] ?? ""));
}

/** States after which a document is closed, for servers that send no guard yet. */
const CLOSED = new Set([
  "posted",
  "cancelled",
  "canceled",
  "reversed",
  "done",
  "paid",
  "closed",
  "locked",
]);

/**
 * Whether the lines of this parent are frozen, and the words why. With the D77 guard in meta
 * (D95), they are frozen while the child declares `children_of = <via>` and the parent's guard
 * freezes it. A server that sends no guard at all: frozen once the parent's governed state has
 * moved on to a closed state (posted, cancelled, reversed, done, paid), never in its first one.
 */
export function linesFrozen(
  parent: ModelMeta,
  parentRow: Row | null,
  child: ModelMeta,
  via: string,
): { frozen: boolean; message?: string } {
  if (!parentRow) return { frozen: false };
  if (parent.guard || child.guard) {
    const frozen = child.guard?.children_of === via && guardFreezes(parent.guard, parentRow);
    return { frozen, message: frozen ? parent.guard?.message : undefined };
  }
  const st = stateFieldOf(parent);
  if (!st || !isGoverned(parent, st)) return { frozen: false };
  const v = String(parentRow[st.name] ?? "");
  return { frozen: v !== initialOption(st) && CLOSED.has(v) };
}

/** Whether one child record is frozen by its own guard (`immutable_when`; `append_only`). */
export function lineFrozen(child: ModelMeta, row: Row | null): boolean {
  return guardFreezes(child.guard, row);
}

/** Where a picked product lands: the first ref column, whose pick fills the others. */
export function fillingRef(model: ModelMeta, via: string): FieldMeta | undefined {
  return model.fields.find((f) => f.kind === "ref" && f.name !== via && f.to);
}

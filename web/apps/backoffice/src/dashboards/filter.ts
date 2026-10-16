import type { FieldMeta, ModelMeta, ServerFilter } from "../api.ts";
import { label } from "../format.ts";
import type { Cond } from "../query.ts";

/**
 * The filter of a dashboard widget is plain text, one condition per `;`:
 *
 *     state = paid; amount_minor >= 100000; title ~ renewal; stage in new|qualified; email is set
 *
 * Operators: `=` `!=` `~` (contains, text only) `>` `>=` `<` `<=` `in` (values separated by `|`),
 * and the word forms `is empty` and `is set`. A value has the field's kind: numbers and money are
 * the stored integers (money in minor units, so 12.50 is 1250), dates are YYYY-MM-DD, booleans are
 * true or false, selects are one of the options, refs are an id.
 *
 * The package documentation also allows a JSON object of exact values, `{"state":"paid"}`, which
 * is read as `state = paid` for each key. Empty text means all records.
 */
export const FILTER_HELP =
  "One condition per semicolon: field operator value. Operators: = != ~ (contains, text only) > >= < <= in (values separated by |), and “is empty” or “is set”. Numbers and money use the stored integers (money in minor units: 12.50 is 1250). Example: state = paid; amount_minor >= 100000";

export type WidgetOp =
  | "eq"
  | "ne"
  | "contains"
  | "gt"
  | "gte"
  | "lt"
  | "lte"
  | "in"
  | "empty"
  | "set";
export type Condition = { field: string; op: WidgetOp; value: string };

export const OP_SYMBOL: Record<WidgetOp, string> = {
  eq: "=",
  ne: "!=",
  contains: "~",
  gt: ">",
  gte: ">=",
  lt: "<",
  lte: "<=",
  in: "in",
  empty: "is empty",
  set: "is set",
};
export const OP_NAME: Record<WidgetOp, string> = {
  eq: "is",
  ne: "is not",
  contains: "contains",
  gt: "is greater than",
  gte: "is at least",
  lt: "is less than",
  lte: "is at most",
  in: "is one of",
  empty: "is empty",
  set: "is set",
};

const SYMBOLS: [string, WidgetOp][] = [
  ["!=", "ne"],
  [">=", "gte"],
  ["<=", "lte"],
  ["=", "eq"],
  [">", "gt"],
  ["<", "lt"],
  ["~", "contains"],
];

export function formatFilter(conds: Condition[]): string {
  return conds
    .map((c) =>
      c.op === "empty" || c.op === "set"
        ? `${c.field} ${OP_SYMBOL[c.op]}`
        : `${c.field} ${OP_SYMBOL[c.op]} ${c.value}`,
    )
    .join("; ");
}

/** Read the text of a widget's filter; an error names the part that was not understood. */
export function parseFilter(text: string): { conds: Condition[] } | { error: string } {
  const t = text.trim();
  if (!t) return { conds: [] };
  if (t.startsWith("{")) {
    try {
      const o = JSON.parse(t) as unknown;
      if (!o || typeof o !== "object" || Array.isArray(o)) throw new Error("not an object");
      return {
        conds: Object.entries(o as Record<string, unknown>).map(([field, v]) => ({
          field,
          op: "eq" as const,
          value: String(v),
        })),
      };
    } catch {
      return { error: "The filter looks like JSON but is not a valid object" };
    }
  }
  const conds: Condition[] = [];
  for (const part of t.split(";")) {
    const p = part.trim();
    if (!p) continue;
    const word = /^([A-Za-z_][A-Za-z0-9_]*)\s+(is\s+empty|is\s+set|in)(?:\s+(.*))?$/i.exec(p);
    if (word) {
      const kind = word[2]!.toLowerCase().replace(/\s+/g, " ");
      const op: WidgetOp = kind === "is empty" ? "empty" : kind === "is set" ? "set" : "in";
      if (op === "in" && !word[3]?.trim()) return { error: `“${p}” needs values after in` };
      conds.push({ field: word[1]!, op, value: (word[3] ?? "").trim() });
      continue;
    }
    const sym = /^([A-Za-z_][A-Za-z0-9_]*)\s*(!=|>=|<=|=|>|<|~)\s*(.*)$/.exec(p);
    if (!sym) return { error: `“${p}” is not field operator value` };
    const op = SYMBOLS.find(([s]) => s === sym[2])![1];
    if (!sym[3]!.trim()) return { error: `“${p}” has no value` };
    conds.push({ field: sym[1]!, op, value: sym[3]!.trim() });
  }
  return { conds };
}

function typed(f: FieldMeta, raw: string): unknown | { error: string } {
  switch (f.kind) {
    case "number":
    case "money":
      return /^-?\d+$/.test(raw)
        ? parseInt(raw, 10)
        : {
            error: `${label(f.name)} takes a whole number${f.kind === "money" ? " in minor units" : ""}`,
          };
    case "bool":
      return raw === "true" || raw === "false"
        ? raw === "true"
        : { error: `${label(f.name)} is true or false` };
    case "date":
      return /^\d{4}-\d{2}-\d{2}$/.test(raw)
        ? raw
        : { error: `${label(f.name)} takes a date like 2026-01-31` };
    case "select":
      return f.options?.includes(raw)
        ? raw
        : { error: `${label(f.name)} must be one of: ${f.options?.join(", ")}` };
    default:
      return raw;
  }
}
const isErr = (v: unknown): v is { error: string } =>
  !!v && typeof v === "object" && "error" in (v as object);

/**
 * Check conditions against the fields the viewer can read and turn them into server filters.
 * `noAccess` is set when a condition names a field this person cannot see (or that does not
 * exist): the widget then shows "No access" and nothing is requested.
 */
export function toServerFilter(
  conds: Condition[],
  model: ModelMeta,
): { filter: ServerFilter[] } | { error: string } | { noAccess: string } {
  const filter: ServerFilter[] = [];
  for (const c of conds) {
    const f = c.field === "id" ? undefined : model.fields.find((x) => x.name === c.field);
    if (!f && c.field !== "id") return { noAccess: c.field };
    if (!f) {
      filter.push({ field: "id", op: c.op === "ne" ? "ne" : "eq", value: c.value });
      continue;
    }
    switch (c.op) {
      case "empty":
        filter.push({ field: f.name, op: "is_null", value: true });
        break;
      case "set":
        filter.push({ field: f.name, op: "is_null", value: false });
        break;
      case "contains":
        if (f.kind !== "text" && f.kind !== "longtext")
          return { error: `${label(f.name)} is not a text field, so ~ (contains) does not apply` };
        filter.push({ field: f.name, op: "contains", value: c.value });
        break;
      case "in": {
        const vals: unknown[] = [];
        for (const raw of c.value.split("|").map((s) => s.trim())) {
          const v = typed(f, raw);
          if (isErr(v)) return v;
          vals.push(v);
        }
        filter.push({ field: f.name, op: "in", value: vals });
        break;
      }
      case "gt":
      case "gte":
      case "lt":
      case "lte": {
        if (f.kind === "bool" || f.kind === "ref")
          return { error: `${label(f.name)} cannot be compared with ${OP_SYMBOL[c.op]}` };
        const v = typed(f, c.value);
        if (isErr(v)) return v;
        filter.push({ field: f.name, op: c.op, value: v });
        break;
      }
      default: {
        const v = typed(f, c.value);
        if (isErr(v)) return v;
        filter.push({ field: f.name, op: c.op, value: v });
      }
    }
  }
  return { filter };
}

const dayAfter = (d: string, delta: number) => {
  const [y, m, day] = d.split("-").map(Number) as [number, number, number];
  const dt = new Date(Date.UTC(y, m - 1, day + delta));
  return dt.toISOString().slice(0, 10);
};

/** The same conditions as list facets, so a click on a tile opens the list narrowed alike. */
export function toFacets(conds: Condition[], model: ModelMeta): Cond[] {
  const out: Cond[] = [];
  for (const c of conds) {
    const f = model.fields.find((x) => x.name === c.field);
    if (!f) continue;
    const minor = (n: number) => (f.kind === "money" ? (n / 100).toFixed(2) : String(n));
    switch (c.op) {
      case "empty":
        out.push({ op: "empty", f: f.name });
        break;
      case "set":
        if (f.kind === "bool") out.push({ op: "bool", f: f.name, v: true });
        else out.push({ op: "isnot", f: f.name, v: [""] });
        break;
      case "contains":
        out.push({ op: "contains", f: f.name, v: c.value });
        break;
      case "eq":
      case "ne":
      case "in": {
        const vals = c.op === "in" ? c.value.split("|").map((s) => s.trim()) : [c.value];
        if (f.kind === "bool")
          out.push({ op: "bool", f: f.name, v: (vals[0] === "true") !== (c.op === "ne") });
        else if (f.kind === "select" || f.kind === "ref")
          out.push({ op: c.op === "ne" ? "isnot" : "is", f: f.name, v: vals });
        else if (c.op === "eq" && f.kind === "date")
          out.push({ op: "range", f: f.name, min: c.value, max: c.value });
        else if (c.op === "eq" && (f.kind === "number" || f.kind === "money"))
          out.push({
            op: "range",
            f: f.name,
            min: minor(Number(c.value)),
            max: minor(Number(c.value)),
          });
        else if (c.op === "eq") out.push({ op: "contains", f: f.name, v: c.value });
        break;
      }
      default: {
        // Strict bounds on whole numbers and days become inclusive ones.
        const n = Number(c.value);
        const gt = c.op === "gt" || c.op === "gte";
        if (f.kind === "date")
          out.push(
            gt
              ? { op: "range", f: f.name, min: c.op === "gt" ? dayAfter(c.value, 1) : c.value }
              : { op: "range", f: f.name, max: c.op === "lt" ? dayAfter(c.value, -1) : c.value },
          );
        else if (f.kind === "number" || f.kind === "money") {
          const adj = c.op === "gt" ? n + 1 : c.op === "lt" ? n - 1 : n;
          out.push(
            gt
              ? { op: "range", f: f.name, min: minor(adj) }
              : { op: "range", f: f.name, max: minor(adj) },
          );
        }
      }
    }
  }
  return out;
}

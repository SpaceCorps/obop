import type { FieldMeta, ServerFilter, ServerOrder } from "./api.ts";
import { validDate } from "./format.ts";
import { rangeNum, type Cond, type ViewState } from "./query.ts";

/**
 * Turns the control panel's search state into a `<model>.search` query (D49).
 *
 * What the server can express is sent to the server. Anything else is reported in `rest`, and the
 * list then falls back to loading the records (up to 5,000) and evaluating the whole state in the
 * browser, which is what kanban, pivot, graph and calendar always do:
 *
 * - `search` (one word across every field): the server has no "any field" condition.
 * - `contains` on a select, ref, number, money, date or bool field: the server's `contains` is
 *   for text and longtext only, and the browser matches the displayed label.
 * - `is` with the "None" value mixed with real values, and `is` on a date or number field.
 * - sorting by a ref field (the browser sorts by the label of the record, the server by the id).
 * - grouping (needs every record).
 * - a range with a bound that is not a valid date or number.
 */
export type ServerPlan = {
  filter: ServerFilter[];
  order: ServerOrder[];
  /** Conditions the server cannot evaluate; empty means the server can answer the whole state. */
  rest: Cond[];
  /** True when the sort is something the server orders the same way as the browser. */
  sortOnServer: boolean;
};

const addMonth = (key: string): string => {
  const [y, m] = key.split("-").map(Number) as [number, number];
  return m === 12 ? `${y + 1}-01-01` : `${y}-${String(m + 1).padStart(2, "0")}-01`;
};

function convert(c: Cond, f: FieldMeta | undefined): ServerFilter[] | null {
  if (c.op === "search") return null;
  if (!f) return []; // a field the person cannot see: the browser ignores it too
  switch (c.op) {
    case "contains":
      return f.kind === "text" || f.kind === "longtext"
        ? [{ field: f.name, op: "contains", value: c.v }]
        : null;
    case "is": {
      if (f.kind === "bool") {
        const want = new Set(c.v);
        if (want.has("true") && want.has("false")) return [];
        return want.has("true")
          ? [{ field: f.name, op: "eq", value: true }]
          : [{ field: f.name, op: "ne", value: true }];
      }
      if (f.kind !== "select" && f.kind !== "ref") return null;
      if (c.v.length === 0 || c.v.includes("")) return null;
      return c.v.length === 1
        ? [{ field: f.name, op: "eq", value: c.v[0] }]
        : [{ field: f.name, op: "in", value: c.v }];
    }
    case "isnot": {
      if (f.kind !== "select" && f.kind !== "ref") return null;
      return c.v.map((v): ServerFilter =>
        v === ""
          ? { field: f.name, op: "is_null", value: false }
          : { field: f.name, op: "ne", value: v },
      );
    }
    case "bool":
      return c.v
        ? [{ field: f.name, op: "eq", value: true }]
        : [{ field: f.name, op: "ne", value: true }];
    case "month":
      if (f.kind !== "date" || !/^\d{4}-(0[1-9]|1[0-2])$/.test(c.v)) return null;
      return [
        { field: f.name, op: "gte", value: `${c.v}-01` },
        { field: f.name, op: "lt", value: addMonth(c.v) },
      ];
    case "empty":
      return [{ field: f.name, op: "is_null", value: true }];
    case "range": {
      const out: ServerFilter[] = [];
      if (f.kind === "date") {
        if ((c.min && !validDate(c.min)) || (c.max && !validDate(c.max))) return null;
        if (c.min) out.push({ field: f.name, op: "gte", value: c.min });
        if (c.max) out.push({ field: f.name, op: "lte", value: c.max });
        return out;
      }
      if (f.kind !== "number" && f.kind !== "money") return null;
      const lo = rangeNum(f, c.min);
      const hi = rangeNum(f, c.max);
      if ((c.min && lo === undefined) || (c.max && hi === undefined)) return null;
      // The server compares integers: round the bounds inwards.
      if (lo !== undefined) out.push({ field: f.name, op: "gte", value: Math.ceil(lo) });
      if (hi !== undefined) out.push({ field: f.name, op: "lte", value: Math.floor(hi) });
      return out;
    }
  }
}

export function toServer(state: ViewState, fields: FieldMeta[]): ServerPlan {
  const filter: ServerFilter[] = [];
  const rest: Cond[] = [];
  for (const c of state.facets) {
    const f = c.op === "search" ? undefined : fields.find((x) => x.name === c.f);
    const out = convert(c, f);
    if (out === null) rest.push(c);
    else filter.push(...out);
  }
  const sf = state.sort && fields.find((x) => x.name === state.sort!.f);
  // A sort on a field the person cannot read is ignored, like the browser does.
  const sortOnServer = !sf || sf.kind !== "ref";
  const order: ServerOrder[] =
    state.sort && sf && sf.kind !== "ref" ? [{ field: sf.name, dir: state.sort.dir }] : [];
  return { filter, order, rest, sortOnServer };
}

/** Whether the list can run entirely on the server: no leftovers, no grouping, a server sort. */
export const onServer = (plan: ServerPlan, state: ViewState) =>
  plan.rest.length === 0 && plan.sortOnServer && state.group.length === 0;

/** Server errors about fields the person may not read look like `unknown field lead.email`. */
export const isWithheldError = (message: string) => /unknown field /.test(message);

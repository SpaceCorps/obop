import { describe, expect, it } from "vite-plus/test";
import type { AggGroup, FieldMeta, Row } from "./api.ts";
import { groupKey } from "./query.ts";
import { toServer } from "./serverQuery.ts";
import { groupByFor, keyOf, measuresFor, valueOfGroup } from "./useAggregate.ts";
import { clientCells, serverCells } from "./views/PivotView.tsx";
import { columnFilter, columnTotals } from "./views/KanbanView.tsx";

const f = (name: string, kind: FieldMeta["kind"], options?: string[]): FieldMeta => ({
  name,
  kind,
  options,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
});
const stage = f("stage", "select", ["new", "qualified", "won"]);
const due = f("due", "date");
const done = f("done", "bool");
const amount = f("amount_minor", "money");
const owner = f("owner_id", "ref");

/** What the server's `aggregate` answers for a model, written the slow obvious way. */
function oracle(rows: Row[], by: FieldMeta[], measure?: FieldMeta): AggGroup[] {
  const key = (r: Row, g: FieldMeta): unknown => {
    const v = r[g.name];
    if (v === null || v === undefined) return null;
    return g.kind === "date" ? String(v).slice(0, 7) : v;
  };
  const groups = new Map<string, AggGroup>();
  for (const r of rows) {
    const k = Object.fromEntries(by.map((g) => [g.name, key(r, g)]));
    const id = JSON.stringify(k);
    const g = groups.get(id) ?? { key: k, count: 0, measures: {} };
    g.count++;
    if (measure) {
      const n = Number(r[measure.name]);
      g.measures[`sum:${measure.name}`] =
        (g.measures[`sum:${measure.name}`] ?? 0) + (Number.isFinite(n) ? n : 0);
    }
    groups.set(id, g);
  }
  return [...groups.values()];
}

function random(n: number, seed = 1): Row[] {
  let x = seed;
  const next = (m: number) => {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    return x % m;
  };
  return Array.from({ length: n }, (_, i) => {
    const r: Row = { id: `r${i}` };
    if (next(5)) r.stage = stage.options![next(3)]!;
    if (next(4))
      r.due = `2026-${String(1 + next(12)).padStart(2, "0")}-${String(1 + next(28)).padStart(2, "0")}`;
    if (next(3)) r.done = next(2) === 0;
    if (next(6)) r.amount_minor = next(100000);
    if (next(3)) r.owner_id = `u${next(4)}`;
    return r;
  });
}

describe("group keys", () => {
  it("turn server values into the strings the views use", () => {
    expect(keyOf(null)).toBe("");
    expect(keyOf(undefined)).toBe("");
    expect(keyOf(true)).toBe("true");
    expect(keyOf(false)).toBe("false");
    expect(keyOf("won")).toBe("won");
    expect(keyOf("2026-10")).toBe("2026-10");
    expect(keyOf(12)).toBe("12");
  });

  it("group dates by month, like the browser does", () => {
    expect(groupByFor(due)).toEqual({ field: "due", bucket: "month" });
    expect(groupByFor(stage)).toEqual({ field: "stage" });
    expect(groupKey(due, { id: "a", due: "2026-10-17" })).toBe("2026-10");
  });

  it("measures: a count, and a sum when a field is chosen", () => {
    expect(measuresFor(undefined)).toEqual([{ op: "count" }]);
    expect(measuresFor(amount)).toEqual([{ op: "count" }, { op: "sum", field: "amount_minor" }]);
    const g: AggGroup = { key: {}, count: 3, measures: { "sum:amount_minor": 50 } };
    expect(valueOfGroup(g, amount)).toBe(50);
    expect(valueOfGroup(g, undefined)).toBe(3);
    expect(valueOfGroup({ key: {}, count: 0, measures: {} }, amount)).toBe(0);
  });
});

describe("pivot cells: server groups equal the browser's own grouping", () => {
  for (const seed of [1, 2, 3]) {
    for (const [name, rowF, colF, m] of [
      ["select x date", stage, due, undefined],
      ["select with a sum", stage, undefined, amount],
      ["bool x ref with a sum", done, owner, amount],
      ["date x select", due, stage, undefined],
    ] as [string, FieldMeta, FieldMeta | undefined, FieldMeta | undefined][]) {
      it(`${name} (seed ${seed})`, () => {
        const rows = random(300, seed);
        const server = serverCells(oracle(rows, colF ? [rowF, colF] : [rowF], m), rowF, colF, m);
        const client = clientCells(rows, rowF, colF, m);
        expect(Object.fromEntries(server)).toEqual(Object.fromEntries(client));
      });
    }
  }
});

describe("kanban columns", () => {
  it("count and sum per column, by currency", () => {
    const groups: AggGroup[] = [
      { key: { stage: "new", currency: "EUR" }, count: 2, measures: { "sum:amount_minor": 300 } },
      { key: { stage: "new", currency: "USD" }, count: 1, measures: { "sum:amount_minor": 50 } },
      { key: { stage: null, currency: null }, count: 4, measures: { "sum:amount_minor": 0 } },
    ];
    const t = columnTotals(groups, stage, amount);
    expect(t.get("new")?.count).toBe(3);
    expect([...t.get("new")!.sums]).toEqual([
      ["EUR", 300],
      ["USD", 50],
    ]);
    expect(t.get("")?.count).toBe(4);
  });

  it("select the cards of one column on the server", () => {
    expect(columnFilter(stage, "won")).toEqual([{ field: "stage", op: "eq", value: "won" }]);
    expect(columnFilter(stage, "")).toEqual([{ field: "stage", op: "is_null", value: true }]);
    expect(columnFilter(done, "false")).toEqual([{ field: "done", op: "eq", value: false }]);
    expect(columnFilter(due, "2026-12")).toEqual([
      { field: "due", op: "gte", value: "2026-12-01" },
      { field: "due", op: "lt", value: "2027-01-01" },
    ]);
    expect(columnFilter(due, "2026-03")[1]).toEqual({
      field: "due",
      op: "lt",
      value: "2026-04-01",
    });
  });

  it("agree with the search state's own conversion of the same column", () => {
    for (const [field, key, cond] of [
      [stage, "won", { op: "is", f: "stage", v: ["won"] }],
      [due, "2026-12", { op: "month", f: "due", v: "2026-12" }],
      [stage, "", { op: "empty", f: "stage" }],
    ] as const) {
      const plan = toServer({ facets: [cond as never], group: [] }, [stage, due]);
      expect(columnFilter(field, key)).toEqual(plan.filter);
    }
  });
});

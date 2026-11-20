// reports.ts: what the pivot, the graph, the tiles and the overviews add up, and how they say it.
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setInstanceDefaults, setLanguage } from "@obop/ui";
import type { AggGroup, FieldMeta, ModelMeta } from "./api.ts";
import { foldOther, niceTicks, OTHER_KEY } from "./charts/Chart.tsx";
import { kpiValues } from "./charts/KpiTile.tsx";
import { indexModels } from "./presentation.ts";
import type { Lookup } from "./query.ts";
import {
  bucketLabel,
  cellValue,
  currenciesOf,
  currencySplit,
  defaultMeasure,
  defaultRows,
  dimCond,
  dimKeyOf,
  dimRef,
  dimensionsOf,
  fillPeriods,
  formatMeasure,
  groupSum,
  matrixOf,
  measureAggs,
  measureFilter,
  measureOptions,
  measureRef,
  measureTitle,
  mondayOf,
  parseDim,
  parseMeasure,
  reportFromGroups,
  reportFromRows,
  type Dim,
} from "./reports.ts";

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en");
  await setInstanceDefaults({ currency: "EUR" });
});

const f = (name: string, kind: FieldMeta["kind"], more: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  description: "",
  class: "none",
  personal_data: false,
  kind,
  required: false,
  ...more,
});
const model = (name: string, fields: FieldMeta[], more: Partial<ModelMeta> = {}): ModelMeta => ({
  name,
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields,
  actions: [],
  ...more,
});
const lk: Lookup = { refText: (_m, id) => `#${id}`, refIds: () => [] };

const state = f("state", "select", {
  options: ["draft", "confirmed", "cancelled"],
  governed: true,
});
const order = model("sale_order", [
  f("number", "text", { sequence: true }),
  f("customer_id", "ref", { to: "party", required: true }),
  f("ordered_on", "date", { required: true }),
  f("total_minor", "money"),
  f("unit_price_minor", "money"),
  f("progress_pct", "number"),
  f("last_seq", "number"),
  f("odometer_km", "number"),
  f("quantity", "number"),
  f("currency", "text"),
  state,
]);

describe("dimensions", () => {
  it("are selects, refs, yes/no, dates and instants, in declared order", () => {
    const m = model("x", [
      f("title", "text"),
      f("at", "datetime"),
      f("kind", "select", { options: ["a"] }),
      f("hidden", "select", { options: ["a"], ui: { hidden: true } }),
      f("ok", "bool"),
    ]);
    expect(dimensionsOf(m.fields).map((x) => x.name)).toEqual(["at", "kind", "ok"]);
  });

  it("read field or field:bucket, a date by month by default, and write a month plainly", () => {
    const dims = dimensionsOf(order.fields);
    expect(parseDim("ordered_on", dims)).toMatchObject({ bucket: "month" });
    expect(parseDim("ordered_on:week", dims)).toMatchObject({ bucket: "week" });
    expect(parseDim("ordered_on:nonsense", dims)).toMatchObject({ bucket: "month" });
    expect(parseDim("state", dims)?.bucket).toBeUndefined();
    expect(parseDim("number", dims)).toBeUndefined();
    expect(dimRef(parseDim("ordered_on", dims)!)).toBe("ordered_on");
    expect(dimRef(parseDim("ordered_on:year", dims)!)).toBe("ordered_on:year");
  });

  it("offer the ISO week the server cuts, the year of its Thursday (K1)", () => {
    const day = f("on", "date");
    const dims = dimensionsOf([day]);
    const iso = parseDim("on:iso_week", dims)!;
    expect(iso.bucket).toBe("iso_week");
    expect(dimRef(iso)).toBe("on:iso_week");
    // The cases of obop-values' own test.
    const key = (on: string) => dimKeyOf(iso, { id: "a", on });
    expect(key("2026-01-01")).toBe("2026-W01");
    expect(key("2024-12-30")).toBe("2025-W01");
    expect(key("2027-01-01")).toBe("2026-W53");
    expect(key("2021-01-03")).toBe("2020-W53");
    expect(key("2026-10-25")).toBe("2026-W43");
    expect(key("2026-10-26")).toBe("2026-W44");
    expect(key("2019-12-30")).toBe("2020-W01");
    expect(bucketLabel("iso_week", "2026-W44")).toBe("Week 44, 2026");
    expect(dimCond(iso, "2026-W44")).toEqual({
      op: "range",
      f: "on",
      min: "2026-10-26",
      max: "2026-11-01",
    });
    expect(fillPeriods(["2026-W52", "2027-W02"], "iso_week")).toEqual([
      "2026-W52",
      "2026-W53",
      "2027-W01",
      "2027-W02",
    ]);
  });

  it("cut records into periods the way the server does, weeks from Monday", () => {
    const at = f("at", "datetime");
    const day = f("on", "date");
    expect(mondayOf("2026-11-15")).toBe("2026-11-09");
    expect(mondayOf("2026-11-09")).toBe("2026-11-09");
    expect(dimKeyOf({ field: day, bucket: "week" }, { id: "a", on: "2026-11-15" })).toBe(
      "2026-11-09",
    );
    expect(dimKeyOf({ field: day, bucket: "year" }, { id: "a", on: "2026-11-15" })).toBe("2026");
    // 23:30 UTC on 31 Oct is already 1 Nov in Berlin.
    const row = { id: "a", at: "2026-10-31T23:30:00Z" };
    expect(dimKeyOf({ field: at, bucket: "month" }, row, "UTC")).toBe("2026-10");
    expect(dimKeyOf({ field: at, bucket: "month" }, row, "Europe/Berlin")).toBe("2026-11");
    expect(dimKeyOf({ field: day, bucket: "day" }, { id: "a" })).toBe("");
  });

  it("drill into exactly one group", () => {
    const day = f("on", "date");
    const at = f("at", "datetime");
    expect(dimCond({ field: day, bucket: "month" }, "2026-02")).toEqual({
      op: "month",
      f: "on",
      v: "2026-02",
    });
    expect(dimCond({ field: at, bucket: "month" }, "2026-02")).toEqual({
      op: "range",
      f: "at",
      min: "2026-02-01",
      max: "2026-02-28",
    });
    expect(dimCond({ field: day, bucket: "week" }, "2026-12-28")).toEqual({
      op: "range",
      f: "on",
      min: "2026-12-28",
      max: "2027-01-03",
    });
    expect(dimCond({ field: day, bucket: "year" }, "2026")).toMatchObject({
      min: "2026-01-01",
      max: "2026-12-31",
    });
    expect(dimCond({ field: state }, "")).toEqual({ op: "empty", f: "state" });
    expect(dimCond({ field: state }, "draft")).toEqual({ op: "is", f: "state", v: ["draft"] });
  });

  it("fill the quiet periods of a series, but not a huge gap", () => {
    expect(fillPeriods(["2026-01", "2026-04"], "month")).toEqual([
      "2026-01",
      "2026-02",
      "2026-03",
      "2026-04",
    ]);
    expect(fillPeriods(["2025-12", "2026-01", ""], "month")).toEqual(["2025-12", "2026-01", ""]);
    expect(fillPeriods(["2026-11-02", "2026-11-16"], "week")).toEqual([
      "2026-11-02",
      "2026-11-09",
      "2026-11-16",
    ]);
    expect(fillPeriods(["2000-01-01", "2026-01-01"], "day")).toEqual(["2000-01-01", "2026-01-01"]);
  });

  it("name periods in words", () => {
    expect(bucketLabel("year", "2026")).toBe("2026");
    expect(bucketLabel("week", "2026-11-09")).toMatch(/^Week of /);
    expect(bucketLabel("month", "")).toBe("None");
  });
});

describe("measures", () => {
  it("sum what adds up, average prices and percentages, and never helpers", () => {
    const o = measureOptions(order, order.fields);
    expect(o.sums.map((x) => x.name)).toEqual(["total_minor", "quantity"]);
    expect(o.avgs.map((x) => x.name)).toEqual(["unit_price_minor", "progress_pct"]);
  });

  it("only count on a configuration model", () => {
    const tax = model("tax", [f("rate_bp", "number"), f("amount", "money")]);
    expect(measureOptions(tax, tax.fields, true)).toEqual({ sums: [], avgs: [] });
  });

  it("follow the aggregate hint", () => {
    const m = model("svc", [
      f("cost", "money", { ui: { aggregate: "sum" } }),
      f("total", "money", { ui: { aggregate: "none" } }),
      f("rating", "decimal", { ui: { aggregate: "avg" } }),
    ]);
    const o = measureOptions(m, m.fields);
    expect(o.sums.map((x) => x.name)).toEqual(["cost"]);
    expect(o.avgs.map((x) => x.name)).toEqual(["rating"]);
  });

  it("round-trip through the address, reading an old sum of an averaged field as its average", () => {
    const o = measureOptions(order, order.fields);
    expect(parseMeasure("count", o)).toBeNull();
    expect(parseMeasure(undefined, o)).toBeUndefined();
    expect(parseMeasure("nope", o)).toBeUndefined();
    expect(parseMeasure("total_minor", o)).toMatchObject({ op: "sum" });
    expect(parseMeasure("unit_price_minor", o)).toMatchObject({ op: "avg" });
    const avg = parseMeasure("avg:unit_price_minor", o)!;
    expect(measureRef(avg)).toBe("avg:unit_price_minor");
    expect(measureRef(null)).toBe("count");
    expect(measureFilter(avg)).toEqual([
      { field: "unit_price_minor", op: "is_null", value: false },
    ]);
    expect(measureTitle(avg)).toBe("Average of Unit price");
  });

  it("open with the hinted measure, else money, else a quantity, else the count", () => {
    const o = measureOptions(order, order.fields);
    expect(defaultMeasure(order, o)?.field.name).toBe("total_minor");
    const hinted = { ...order, ui: { report: { measure: "quantity" } } };
    expect(defaultMeasure(hinted, o)?.field.name).toBe("quantity");
    const stock = model("item", [f("quantity", "number"), f("reorder_level", "number")]);
    expect(defaultMeasure(stock, measureOptions(stock, stock.fields))?.field.name).toBe("quantity");
    const plain = model("note", [f("title", "text")]);
    expect(defaultMeasure(plain, measureOptions(plain, plain.fields))).toBeNull();
  });

  it("show minutes as hours, metres as km and an average as sum over count", () => {
    const minutes = f("minutes", "number");
    expect(formatMeasure({ field: minutes, op: "sum" }, 90)).toMatch(/1\.5/);
    const dist = f("distance_m", "decimal", { scale: 2 });
    expect(formatMeasure({ field: dist, op: "sum" }, 165142.69)).toMatch(/165\.1/);
    expect(formatMeasure({ field: order.fields[3]!, op: "sum" }, 12550, "EUR")).toBe("€125.50");
    expect(formatMeasure(null, 7)).toBe("7");
    const avg = { field: order.fields[4]!, op: "avg" as const };
    expect(cellValue(avg, { count: 4, value: 1000 })).toBe(250);
    expect(cellValue(null, { count: 4, value: 1000 })).toBe(4);
  });
});

describe("defaults", () => {
  const party = model("party", [f("name", "text", { required: true })]);
  const move = model("accounting_move", [
    f("journal_id", "ref", { to: "accounting_journal", required: true }),
  ]);
  const line = model("accounting_move_line", [
    f("move_id", "ref", { to: "accounting_move", required: true }),
    f("account_id", "ref", { to: "accounting_account", required: true }),
    f("debit", "money"),
  ]);
  const index = indexModels([{ id: "a", name: "A", models: [party, move, line] } as never]);

  it("rows: the hint, the main ref but not the parent of a line, a date by month, the status", () => {
    expect(defaultRows(order, dimensionsOf(order.fields))?.field.name).toBe("customer_id");
    expect(defaultRows(line, dimensionsOf(line.fields), index)?.field.name).toBe("account_id");
    // A child with its own date is an event: its parent is the group (services by vehicle).
    const event = { ...line, fields: [...line.fields, f("booked_on", "date")] };
    expect(defaultRows(event, dimensionsOf(event.fields), index)?.field.name).toBe("move_id");
    const dated = model("x", [f("on", "date"), state]);
    expect(defaultRows(dated, dimensionsOf(dated.fields))).toMatchObject({ bucket: "month" });
    const onlyState = model("y", [state]);
    expect(defaultRows(onlyState, dimensionsOf(onlyState.fields))?.field.name).toBe("state");
    const hinted = { ...order, ui: { report: { rows: "ordered_on:week" } } };
    expect(defaultRows(hinted, dimensionsOf(order.fields))).toMatchObject({ bucket: "week" });
  });
});

describe("reports and currencies", () => {
  const amount = order.fields[3]!;
  const sum = { field: amount, op: "sum" as const };
  const dim: Dim = { field: state };

  it("split money by the record's currency, else the first step of currency_from", () => {
    expect(currencySplit(order, sum)).toEqual({ field: "currency" });
    expect(currencySplit(order, null)).toBeNull();
    const line = model("move_line", [
      f("journal_id", "ref", { to: "journal" }),
      f("debit", "money", { ui: { currency_from: "journal_id.currency" } }),
    ]);
    expect(currencySplit(line, { field: line.fields[1]!, op: "sum" })).toEqual({
      field: "journal_id",
      path: "journal_id.currency",
    });
    const bare = model("bare", [f("amount", "money")]);
    expect(currencySplit(bare, { field: bare.fields[0]!, op: "sum" })).toBeNull();
  });

  it("add up groups that land in the same currency, and keep currencies apart (B11)", () => {
    const groups: (AggGroup & { split?: unknown })[] = [
      { key: { state: "draft" }, count: 2, measures: { "sum:total_minor": 100 }, split: "EUR" },
      { key: { state: "draft" }, count: 1, measures: { "sum:total_minor": 50 }, split: null },
      { key: { state: "draft" }, count: 1, measures: { "sum:total_minor": 70 }, split: "SEK" },
    ];
    const toCur = (v: unknown) => (typeof v === "string" ? v : "EUR");
    const rows = reportFromGroups(groups, [dim], sum, toCur);
    expect(rows).toEqual([
      { keys: ["draft"], currency: "EUR", count: 3, value: 150 },
      { keys: ["draft"], currency: "SEK", count: 1, value: 70 },
    ]);
    expect(currenciesOf(rows)).toEqual(["EUR", "SEK"]);
    // A count never splits.
    const counted = reportFromGroups(groups, [dim], null, toCur);
    expect(counted).toEqual([{ keys: ["draft"], currency: undefined, count: 4, value: 4 }]);
  });

  it("lay out a pivot with margins per currency, an average's margins exact", () => {
    const rows = [
      { keys: ["draft", "a"], currency: "EUR", count: 1, value: 100 },
      { keys: ["draft", "b"], currency: "EUR", count: 3, value: 300 },
      { keys: ["confirmed", "a"], currency: "SEK", count: 1, value: 999 },
    ];
    const col: Dim = { field: f("region", "select", { options: ["a", "b"] }) };
    const mx = matrixOf(rows, [dim, col], lk);
    expect(mx.rowKeys).toEqual(["draft", "confirmed"]);
    expect(mx.colKeys).toEqual(["a", "b"]);
    expect(mx.currencies).toEqual(["EUR", "SEK"]);
    expect(mx.rowTotal("draft", "EUR")).toEqual({ count: 4, value: 400 });
    expect(mx.rowTotal("draft", "SEK")).toEqual({ count: 0, value: 0 });
    expect(mx.colTotal("a", "SEK")).toEqual({ count: 1, value: 999 });
    expect(mx.grand("EUR")).toEqual({ count: 4, value: 400 });
    // The average of the draft row is 400 / 4, not the mean of 100 and 100.
    expect(cellValue({ field: amount, op: "avg" }, mx.rowTotal("draft", "EUR"))).toBe(100);
  });

  it("add up records in the browser like the server, skipping empty values for an average", () => {
    const recs = [
      { id: "1", state: "draft", total_minor: 100, currency: "EUR" },
      { id: "2", state: "draft", currency: "EUR" },
      { id: "3", state: "draft", total_minor: 40, currency: "SEK" },
    ];
    const cur = (r: Record<string, unknown>) => r.currency as string;
    expect(reportFromRows(recs, [dim], sum, { currencyOf: cur })).toEqual([
      { keys: ["draft"], currency: "EUR", count: 2, value: 100 },
      { keys: ["draft"], currency: "SEK", count: 1, value: 40 },
    ]);
    const avg = reportFromRows(recs, [dim], { field: amount, op: "avg" }, { currencyOf: cur });
    expect(avg[0]).toMatchObject({ count: 1, value: 100 });
  });
});

describe("key figures", () => {
  const amount = f("total_minor", "money");
  it("show the instance's currency first and the others beside it", () => {
    const rows = [
      { keys: [], currency: "SEK", count: 1, value: 70000 },
      { keys: [], currency: "EUR", count: 3, value: 12550 },
    ];
    const k = kpiValues(rows, { field: amount, op: "sum" });
    expect(k.value).toBe("€125.50");
    expect(k.more).toHaveLength(1);
    expect(k.more[0]).toMatch(/^SEK\s700\.00$/);
  });
  it("count records, and say zero when there are none", () => {
    expect(kpiValues([{ keys: [], count: 7, value: 7 }], null)).toEqual({ value: "7", more: [] });
    expect(kpiValues([], { field: amount, op: "sum" })).toEqual({ value: "€0.00", more: [] });
  });
});

describe("chart helpers", () => {
  it("round the axis and always include zero, below it too", () => {
    expect(niceTicks(0, 7.3, 4)).toEqual({ lo: 0, hi: 8, ticks: [0, 2, 4, 6, 8] });
    const neg = niceTicks(-120, 380, 4);
    expect(neg.ticks).toContain(0);
    expect(neg.lo).toBeLessThanOrEqual(-120);
    expect(neg.hi).toBeGreaterThanOrEqual(380);
    expect(niceTicks(0, 0)).toEqual({ lo: 0, hi: 1, ticks: [0, 1] });
  });

  it("keep the largest groups and add the rest up as Other", () => {
    const data = Array.from({ length: 12 }, (_, i) => ({
      key: `k${i}`,
      label: `K${i}`,
      value: i + 1,
    }));
    const folded = foldOther(data, 8);
    expect(folded).toHaveLength(9);
    expect(folded[0]!.value).toBe(12);
    expect(folded[8]).toMatchObject({ key: OTHER_KEY, value: 1 + 2 + 3 + 4 });
    expect(foldOther(data.slice(0, 9), 8)).toHaveLength(9);
  });
});

// The server has the smallest and the largest value of a field (K1): the pivot and the graph offer
// them for every field they sum or average, and their margins take the smallest or the largest cell.
describe("the smallest and the largest value", () => {
  const amount = order.fields[3]!;
  const opts = measureOptions(order, order.fields);
  const dim: Dim = { field: state };
  const min = { field: amount, op: "min" } as const;
  const max = { field: amount, op: "max" } as const;

  it("are read from and written to the address, for a summed or an averaged field", () => {
    expect(parseMeasure("min:total_minor", opts)).toEqual(min);
    expect(parseMeasure("max:unit_price_minor", opts)).toEqual({
      field: order.fields[4],
      op: "max",
    });
    expect(parseMeasure("min:nothing", opts)).toBeUndefined();
    expect(measureRef(min)).toBe("min:total_minor");
    expect(measureRef(max)).toBe("max:total_minor");
    expect(measureTitle(min)).toBe("Smallest Total");
    expect(measureTitle(max)).toBe("Largest Total");
  });

  it("are asked of the server by name, without the groups that have no value", () => {
    expect(measureAggs(min)).toEqual([{ op: "count" }, { op: "min", field: "total_minor" }]);
    expect(measureFilter(max)).toEqual([{ field: "total_minor", op: "is_null", value: false }]);
    const g: AggGroup = {
      key: { state: "draft" },
      count: 2,
      measures: { "min:total_minor": 50, "sum:total_minor": 150 },
    };
    expect(groupSum(g, min)).toBe(50);
  });

  it("take the smallest or the largest of their groups, in the pivot margins too", () => {
    const groups: AggGroup[] = [
      { key: { state: "draft" }, count: 1, measures: { "min:total_minor": 300 } },
      { key: { state: "draft" }, count: 2, measures: { "min:total_minor": 100 } },
      { key: { state: "confirmed" }, count: 1, measures: { "min:total_minor": 700 } },
    ];
    const rows = reportFromGroups(groups, [dim], min);
    expect(rows).toEqual([
      { keys: ["draft"], currency: undefined, count: 3, value: 100 },
      { keys: ["confirmed"], currency: undefined, count: 1, value: 700 },
    ]);
    const mx = matrixOf(rows, [dim], lk, false, min);
    expect(cellValue(min, mx.grand(undefined))).toBe(100);
    const big = matrixOf(rows, [dim], lk, false, max);
    expect(cellValue(max, big.grand(undefined))).toBe(700);
    // Records in the browser: an empty value is no value, not a zero.
    const recs = [
      { id: "1", state: "draft", total_minor: 40 },
      { id: "2", state: "draft" },
      { id: "3", state: "draft", total_minor: 90 },
    ];
    expect(reportFromRows(recs, [dim], min)).toEqual([
      { keys: ["draft"], currency: undefined, count: 2, value: 40 },
    ]);
    expect(reportFromRows(recs, [dim], max)[0]!.value).toBe(90);
  });
});

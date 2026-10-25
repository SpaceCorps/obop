import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, Row } from "./api.ts";
import { fromEdit, toEdit } from "./edit.ts";
import {
  matchCond,
  rangeDecimal,
  sortRows,
  sumDecimalRows,
  validRange,
  type Cond,
  type Lookup,
} from "./query.ts";
import { toServer } from "./serverQuery.ts";

const f = (name: string, kind: FieldMeta["kind"], extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const price = f("price", "decimal", { precision: 12, scale: 2 });
const whole = f("count", "decimal", { precision: 6, scale: 0 });
const at = f("booked_at", "datetime");
const fields = [price, at];
const lk: Lookup = { refText: (_m, id) => id, refIds: () => [] };
const row = (v: Partial<Row>): Row => ({ id: "r", ...v });

describe("decimal fields in forms", () => {
  it("turn what was typed into the canonical text, or say why not", () => {
    for (const [typed, want] of [
      ["12.5", "12.50"],
      ["12", "12.00"],
      ["-0.05", "-0.05"],
      ["0", "0.00"],
      ["-0", "0.00"],
      ["007.10", "7.10"],
      ["1234.56", "1234.56"],
    ] as const)
      expect(fromEdit(price, typed), typed).toEqual({ value: want });
    expect(fromEdit(whole, "42")).toEqual({ value: "42" });
    for (const bad of ["1.234", "abc", "1e3", "1..2", "99999999999.00"])
      expect(fromEdit(price, bad), bad).toHaveProperty("error");
    expect(fromEdit(whole, "1.5")).toHaveProperty("error");
    expect(fromEdit(price, "  ")).toEqual({ value: null });
    expect(fromEdit({ ...price, required: true }, "")).toHaveProperty("error");
    expect(toEdit(price, "12.50")).toBe("12.50");
    expect(toEdit(price, null)).toBe("");
  });
});

describe("datetime fields in forms", () => {
  it("read the wall clock of the instance's zone (UTC until /api/meta says otherwise)", () => {
    expect(fromEdit(at, "2026-03-29 03:30")).toEqual({ value: "2026-03-29T03:30:00.000000Z" });
    expect(fromEdit(at, "2026-03-29 03:30+02:00")).toEqual({
      value: "2026-03-29T01:30:00.000000Z",
    });
    expect(fromEdit(at, "2026-02-30 10:00")).toHaveProperty("error");
    expect(fromEdit(at, "yesterday")).toHaveProperty("error");
    expect(toEdit(at, "2026-03-29T01:30:00.000000Z")).toBe("2026-03-29 01:30");
  });
});

describe("range filters on decimals and instants", () => {
  const range = (name: string, min?: string, max?: string): Cond => ({
    op: "range",
    f: name,
    ...(min !== undefined ? { min } : {}),
    ...(max !== undefined ? { max } : {}),
  });

  it("compare decimals by value in the browser", () => {
    const rows = ["9.50", "10.25", "-3.00", "100.00"].map((p) => row({ price: p }));
    const keep = (c: Cond) => rows.filter((r) => matchCond(c, r, fields, lk)).map((r) => r.price);
    expect(keep(range("price", "9.7", "50"))).toEqual(["10.25"]);
    expect(keep(range("price", "9.5"))).toEqual(["9.50", "10.25", "100.00"]);
    expect(keep(range("price", undefined, "0"))).toEqual(["-3.00"]);
    expect(matchCond(range("price", "1"), row({}), fields, lk)).toBe(false);
  });

  it("compare instants by their day in the zone", () => {
    const r = row({ booked_at: "2026-03-31T22:30:00.000000Z" });
    expect(matchCond(range("booked_at", "2026-03-31", "2026-03-31"), r, fields, lk)).toBe(true);
    expect(matchCond(range("booked_at", "2026-04-01"), r, fields, lk)).toBe(false);
  });

  it("go to the server as exact values and as days of the zone", () => {
    expect(
      toServer({ facets: [range("price", "9,5", "10.25")], group: [] }, fields).filter,
    ).toEqual([
      { field: "price", op: "gte", value: "9.5" },
      { field: "price", op: "lte", value: "10.25" },
    ]);
    const plan = toServer(
      { facets: [range("booked_at", "2026-04-01", "2026-04-30")], group: [] },
      fields,
    );
    expect(plan.rest).toEqual([]);
    expect(plan.filter).toEqual([
      { field: "booked_at", op: "gte", bucket: "day", value: "2026-04-01" },
      { field: "booked_at", op: "lte", bucket: "day", value: "2026-04-30" },
    ]);
    // A bound that is not a decimal stays in the browser.
    expect(toServer({ facets: [range("price", "abc")], group: [] }, fields).rest).toHaveLength(1);
  });

  it("validate their bounds", () => {
    const c = (name: string, min?: string, max?: string) =>
      range(name, min, max) as Extract<Cond, { op: "range" }>;
    expect(validRange(price, c("price", "1.5"))).toBe(true);
    expect(validRange(price, c("price", "x"))).toBe(false);
    expect(validRange(at, c("booked_at", "2026-04-01"))).toBe(true);
    expect(validRange(at, c("booked_at", "April"))).toBe(false);
    expect(rangeDecimal("1,5")).toBe("1.5");
    expect(rangeDecimal("1e3")).toBeUndefined();
  });
});

describe("sorting and totals", () => {
  it("sorts decimals as numbers and instants as time", () => {
    const rows = ["10.25", "9.50", "-3.00"].map((p, i) => ({ id: `r${i}`, price: p }));
    const asc = sortRows(rows, { f: "price", dir: "asc" }, fields, lk).map((r) => r.price);
    expect(asc).toEqual(["-3.00", "9.50", "10.25"]);
    const times = ["2026-03-29T01:30:00.000000Z", "2025-01-01T00:00:00.000000Z"].map((v, i) => ({
      id: `t${i}`,
      booked_at: v,
    }));
    const sorted = sortRows(times, { f: "booked_at", dir: "asc" }, fields, lk);
    expect(sorted[0]!.booked_at).toBe("2025-01-01T00:00:00.000000Z");
  });

  it("adds a decimal column exactly", () => {
    const rows = ["0.10", "0.20", "9.99"].map((p, i) => ({ id: `r${i}`, price: p }));
    expect(sumDecimalRows(rows, "price")).toBe("10.29");
  });
});

import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, ModelMeta } from "../api.ts";
import { formatFilter, parseFilter, toFacets, toServerFilter } from "./filter.ts";

const f = (name: string, kind: FieldMeta["kind"], options?: string[]): FieldMeta => ({
  name,
  kind,
  options,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
});
const model: ModelMeta = {
  name: "deal",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  actions: [],
  fields: [
    f("title", "text"),
    f("state", "select", ["draft", "paid"]),
    f("amount_minor", "money"),
    f("due", "date"),
    f("done", "bool"),
    f("rate", "decimal"),
    f("seen_at", "datetime"),
  ],
};

const ok = (text: string) => {
  const r = parseFilter(text);
  if ("error" in r) throw new Error(r.error);
  return r.conds;
};

describe("widget filter text", () => {
  it("is empty for no text", () => {
    expect(ok("")).toEqual([]);
    expect(ok("  ")).toEqual([]);
  });

  it("reads field operator value, separated by semicolons", () => {
    expect(ok("state = paid; amount_minor >= 100000; title ~ renewal")).toEqual([
      { field: "state", op: "eq", value: "paid" },
      { field: "amount_minor", op: "gte", value: "100000" },
      { field: "title", op: "contains", value: "renewal" },
    ]);
    expect(ok("state != draft")[0]).toMatchObject({ op: "ne" });
    expect(ok("state in draft|paid")[0]).toMatchObject({ op: "in", value: "draft|paid" });
    expect(ok("due is empty; title is set")).toEqual([
      { field: "due", op: "empty", value: "" },
      { field: "title", op: "set", value: "" },
    ]);
  });

  it("also reads the JSON object of exact values the package documents", () => {
    expect(ok('{"state":"paid","done":true}')).toEqual([
      { field: "state", op: "eq", value: "paid" },
      { field: "done", op: "eq", value: "true" },
    ]);
    expect(parseFilter("{nope")).toHaveProperty("error");
  });

  it("names the part it did not understand", () => {
    expect(parseFilter("state is nonsense")).toEqual({
      error: "“state is nonsense” is not field operator value",
    });
    expect(parseFilter("state =")).toHaveProperty("error");
  });

  it("formats back to the same text", () => {
    const text = "state = paid; amount_minor >= 100000; due is empty";
    expect(formatFilter(ok(text))).toBe(text);
  });
});

describe("widget filter to the server", () => {
  const server = (text: string) => toServerFilter(ok(text), model);

  it("tests a decimal or an instant only for being empty or set, as the form offers", () => {
    expect(server("rate is set; seen_at is empty")).toEqual({
      filter: [
        { field: "rate", op: "is_null", value: false },
        { field: "seen_at", op: "is_null", value: true },
      ],
    });
    // Any other operator would count on the server what the list behind the tile cannot show.
    for (const text of [
      "rate >= 5.5",
      "rate = 5.50",
      "seen_at > 2026-03-29T00:00:00Z",
      "rate in 1|2",
    ])
      expect(server(text), text).toHaveProperty("error");
    expect(toServerFilter(ok('{"rate":"5.50"}'), model)).toHaveProperty("error");
  });

  it("types the values by the field's kind", () => {
    expect(server("amount_minor >= 100000; done = true; state in draft|paid")).toEqual({
      filter: [
        { field: "amount_minor", op: "gte", value: 100000 },
        { field: "done", op: "eq", value: true },
        { field: "state", op: "in", value: ["draft", "paid"] },
      ],
    });
  });

  it("refuses values of the wrong kind, with the field's name in the message", () => {
    expect(server("amount_minor = 12.5")).toMatchObject({
      error: expect.stringContaining("whole number"),
    });
    expect(server("state = missing")).toMatchObject({
      error: expect.stringContaining("one of: draft, paid"),
    });
    expect(server("title > 3")).toEqual({ filter: [{ field: "title", op: "gt", value: "3" }] });
    expect(server("done > true")).toHaveProperty("error");
    expect(server("amount_minor ~ x")).toMatchObject({
      error: expect.stringContaining("not a text field"),
    });
  });

  it("a field the person cannot see is reported as no access, not as an error", () => {
    expect(server("secret = 1")).toEqual({ noAccess: "secret" });
  });
});

describe("widget filter to list facets", () => {
  it("opens the list with the same conditions", () => {
    const facets = toFacets(
      ok("state = paid; amount_minor >= 100000; done = true; title ~ x"),
      model,
    );
    expect(facets).toEqual([
      { op: "is", f: "state", v: ["paid"] },
      { op: "range", f: "amount_minor", min: "1000.00" },
      { op: "bool", f: "done", v: true },
      { op: "contains", f: "title", v: "x" },
    ]);
  });

  it("a strict bound becomes an inclusive one on whole numbers and days", () => {
    expect(toFacets(ok("amount_minor > 100"), model)).toEqual([
      { op: "range", f: "amount_minor", min: "1.01" },
    ]);
    expect(toFacets(ok("due < 2026-03-01"), model)).toEqual([
      { op: "range", f: "due", max: "2026-02-28" },
    ]);
  });
});

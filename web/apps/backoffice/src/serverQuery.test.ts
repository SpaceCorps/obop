import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta } from "./api.ts";
import type { Cond, ViewState } from "./query.ts";
import { isWithheldError, onServer, toServer } from "./serverQuery.ts";

const f = (name: string, kind: FieldMeta["kind"], options?: string[]): FieldMeta => ({
  name,
  kind,
  options,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
});
const fields = [
  f("title", "text"),
  f("stage", "select", ["new", "won"]),
  f("amount_minor", "money"),
  f("qty", "number"),
  f("due", "date"),
  f("done", "bool"),
  f("party_id", "ref"),
];
const state = (facets: Cond[], extra: Partial<ViewState> = {}): ViewState => ({
  facets,
  group: [],
  ...extra,
});

describe("search state to server query", () => {
  it("converts what the server can express", () => {
    const plan = toServer(
      state([
        { op: "contains", f: "title", v: "acme" },
        { op: "is", f: "stage", v: ["new"] },
        { op: "isnot", f: "stage", v: ["won"] },
        { op: "empty", f: "due" },
      ]),
      fields,
    );
    expect(plan.rest).toEqual([]);
    expect(plan.filter).toEqual([
      { field: "title", op: "contains", value: "acme" },
      { field: "stage", op: "eq", value: "new" },
      { field: "stage", op: "ne", value: "won" },
      { field: "due", op: "is_null", value: true },
    ]);
  });

  it("several values become `in`", () => {
    const plan = toServer(state([{ op: "is", f: "stage", v: ["new", "won"] }]), fields);
    expect(plan.filter).toEqual([{ field: "stage", op: "in", value: ["new", "won"] }]);
  });

  it("a month is a half-open date range, across a year end too", () => {
    expect(toServer(state([{ op: "month", f: "due", v: "2026-03" }]), fields).filter).toEqual([
      { field: "due", op: "gte", value: "2026-03-01" },
      { field: "due", op: "lt", value: "2026-04-01" },
    ]);
    expect(toServer(state([{ op: "month", f: "due", v: "2026-12" }]), fields).filter[1]).toEqual({
      field: "due",
      op: "lt",
      value: "2027-01-01",
    });
  });

  it("ranges use the field's integers: money in minor units, whole numbers rounded inwards", () => {
    expect(
      toServer(state([{ op: "range", f: "amount_minor", min: "10.50", max: "20" }]), fields).filter,
    ).toEqual([
      { field: "amount_minor", op: "gte", value: 1050 },
      { field: "amount_minor", op: "lte", value: 2000 },
    ]);
    expect(
      toServer(state([{ op: "range", f: "qty", min: "1.5", max: "9.5" }]), fields).filter,
    ).toEqual([
      { field: "qty", op: "gte", value: 2 },
      { field: "qty", op: "lte", value: 9 },
    ]);
    expect(toServer(state([{ op: "range", f: "due", min: "2026-01-01" }]), fields).filter).toEqual([
      { field: "due", op: "gte", value: "2026-01-01" },
    ]);
  });

  it("false is `ne true`, because the server's eq never matches an empty value", () => {
    expect(toServer(state([{ op: "bool", f: "done", v: false }]), fields).filter).toEqual([
      { field: "done", op: "ne", value: true },
    ]);
    expect(toServer(state([{ op: "bool", f: "done", v: true }]), fields).filter).toEqual([
      { field: "done", op: "eq", value: true },
    ]);
  });

  it("leaves the rest to the browser: any-field search, contains on a select, mixed None", () => {
    const search: Cond = { op: "search", v: "x" };
    const sel: Cond = { op: "contains", f: "stage", v: "ne" };
    const none: Cond = { op: "is", f: "stage", v: ["", "new"] };
    const plan = toServer(state([search, sel, none]), fields);
    expect(plan.rest).toEqual([search, sel, none]);
    expect(onServer(plan, state([search]))).toBe(false);
  });

  it("sorts on the server except by a ref, and ignores a sort on a field that is not there", () => {
    expect(toServer(state([], { sort: { f: "amount_minor", dir: "desc" } }), fields).order).toEqual(
      [{ field: "amount_minor", dir: "desc" }],
    );
    const byRef = toServer(state([], { sort: { f: "party_id", dir: "asc" } }), fields);
    expect(byRef.sortOnServer).toBe(false);
    const unknown = toServer(state([], { sort: { f: "gone", dir: "asc" } }), fields);
    expect(unknown.sortOnServer).toBe(true);
    expect(unknown.order).toEqual([]);
  });

  it("grouping needs every record, so the list stays in the browser", () => {
    const s = state([], { group: ["stage"] });
    expect(onServer(toServer(s, fields), s)).toBe(false);
    expect(onServer(toServer(state([]), fields), state([]))).toBe(true);
  });

  it("a condition on a field the person cannot see is dropped, like the browser does", () => {
    expect(toServer(state([{ op: "contains", f: "secret", v: "x" }]), fields)).toMatchObject({
      filter: [],
      rest: [],
    });
  });

  it("recognises the server's refusal to filter on a withheld field", () => {
    expect(isWithheldError("unknown field party.name")).toBe(true);
    expect(isWithheldError('unknown op "like"')).toBe(false);
  });
});

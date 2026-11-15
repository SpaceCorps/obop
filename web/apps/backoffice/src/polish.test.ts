// The fixes of the polish pass after wave 1: cards keep what `ui.card` lists, sums read in their
// own unit, saved filters read bare option names and null, action labels name their noun once,
// a short first column takes no slack, and a picker never shows a salary.
import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";
import { parseWhere, quickFilters } from "./filters.ts";
import { parseRefFilter } from "./inputs/form.ts";
import { cardLayout, pickerSecondLine, toneClassOf } from "./presentation.ts";
import { actionLabelFor, sectionsOf } from "./recordLayout.ts";
import { boardLayout, columnTotals, currencyGroupOf, sumText } from "./views/board.ts";
import { fitColumns, measureColumn, trimColumns } from "./views/listLayout.ts";

const f = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const model = (fields: FieldMeta[], extra: Partial<ModelMeta> = {}, actions: Command[] = []) =>
  ({
    name: "m",
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields,
    actions,
    ...extra,
  }) as ModelMeta;

describe("cards", () => {
  it("draw every field ui.card lists that no slot takes, in hint order", () => {
    const m = model(
      [
        f("name", { required: true }),
        f("employee"),
        f("quantity", { kind: "number" }),
        f("billable", { kind: "bool" }),
        f("price", { kind: "money" }),
      ],
      { ui: { card: ["name", "employee", "quantity", "billable", "price"] } },
    );
    const c = cardLayout(m);
    expect(c.money).toBe("price");
    expect(c.facts).toEqual(["employee", "quantity", "billable"]);
  });

  it("show a number card_sum as the figure, never as money", () => {
    const m = model(
      [
        f("name", { required: true }),
        f("minutes", { kind: "number", ui: { display: "duration_minutes" } }),
      ],
      { ui: { card: ["name", "minutes"], card_sum: "minutes" } },
    );
    const b = boardLayout(m, m.fields);
    expect(b.money).toBeUndefined();
    expect(b.figure).toBe("minutes");
    expect(b.facts).toEqual([]);
  });
});

describe("column sums", () => {
  const group = f("state", { kind: "select", options: ["a", "b"] });
  it("of a number have one sum and read in the field's unit", () => {
    const minutes = f("minutes", { kind: "number", ui: { display: "duration_minutes" } });
    const t = columnTotals(
      [
        { key: { state: "a", currency: "SEK" }, count: 2, measures: { "sum:minutes": 3000 } },
        { key: { state: "a" }, count: 1, measures: { "sum:minutes": 300 } },
      ],
      group,
      minutes,
      "EUR",
    );
    const sums = t.get("a")!.sums;
    expect([...sums.keys()]).toEqual([""]);
    expect(sumText(sums, minutes)).toMatch(/55/);
    expect(sumText(new Map([["", 11]]), f("days", { kind: "number" }))).toMatch(/^11 days$/);
  });

  it("of money follow the currency of the journal (ui.currency_from)", () => {
    const amount = f("amount", { kind: "money", ui: { currency_from: "journal_id.currency" } });
    const m = model([f("journal_id", { kind: "ref", to: "journal" }), amount]);
    expect(currencyGroupOf(m, amount)).toEqual({ field: "journal_id", to: "journal" });
    const cur: Record<string, string> = { j1: "EUR", j2: "SEK" };
    const t = columnTotals(
      [
        { key: { state: "a", journal_id: "j1" }, count: 6, measures: { "sum:amount": -2350114 } },
        { key: { state: "a", journal_id: "j2" }, count: 2, measures: { "sum:amount": -6643825 } },
      ],
      group,
      amount,
      "EUR",
      (k) => cur[String(k.journal_id)],
    );
    expect(Object.fromEntries(t.get("a")!.sums)).toEqual({ EUR: -2350114, SEK: -6643825 });
  });
});

describe("saved filters", () => {
  const m = model([
    f("state", { kind: "select", options: ["draft", "in_review", "closed"] }),
    f("employee_id", { kind: "ref", to: "hr_employee" }),
  ]);
  const ctx = { today: "2026-11-15", user: "root" };
  it("read a bare option name and null", () => {
    expect(parseWhere("state = in_review", m, ctx)).toEqual([
      { op: "is", f: "state", v: ["in_review"] },
    ]);
    expect(parseWhere("state != closed", m, ctx)).toEqual([
      { op: "isnot", f: "state", v: ["closed"] },
    ]);
    expect(parseWhere("employee_id = null", m, ctx)).toEqual([{ op: "empty", f: "employee_id" }]);
    expect(parseWhere("employee_id != null", m, ctx)).toEqual([
      { op: "isnot", f: "employee_id", v: [""] },
    ]);
  });

  it("leave out a generated filter a saved one covers", () => {
    const owned = model([f("assignee"), f("state", { kind: "select", options: ["a"] })], {
      ui: { filter: [{ label: "Mine", where: 'assignee = "root"' }] },
    });
    const labels = quickFilters(owned, owned.fields, ctx).map((q) => q.label);
    expect(labels.filter((l) => l === "Mine")).toHaveLength(1);
  });

  it("of a picker read a bare option name (ref_filter state = open)", () => {
    expect(parseRefFilter("state = open", () => undefined)).toEqual([
      { field: "state", op: "eq", value: "open" },
    ]);
  });
});

describe("action labels", () => {
  const cmd = (id: string): Command => ({ id, description: "", needs_approval: false });
  it("add the noun to a bare verb only", () => {
    expect(actionLabelFor(cmd("pos_session.close"), "session")).not.toMatch(/session session/i);
    const close = { ...cmd("helpdesk_ticket.close"), labels: { en: "Close ticket" } } as Command;
    expect(actionLabelFor(close, "ticket")).not.toMatch(/ticket ticket/i);
  });
});

describe("list columns", () => {
  it("give a short first column no slack", () => {
    const cols = [
      f("reference", { sequence: true }),
      f("description"),
      f("quantity", { kind: "number" }),
    ];
    const { widths } = fitColumns(cols, 1400, (x) => x.name);
    expect(widths[0]).toBeLessThan(widths[1]!);
  });

  it("keep the measure of a list without money as the width shrinks", () => {
    const minutes = f("minutes", { kind: "number", ui: { display: "duration_minutes" } });
    const cols = [f("day", { kind: "date" }), f("task"), f("worker"), minutes];
    const m = model(cols);
    expect(measureColumn(m, cols)).toBe(minutes);
    expect(trimColumns(cols, 3, undefined, minutes)).toContain(minutes);
  });
});

describe("pickers", () => {
  it("never show a salary as the second line", () => {
    const m = model([
      f("name", { required: true }),
      f("salary", { kind: "money", class: "financial", personal_data: true }),
    ]);
    expect(pickerSecondLine(m, { id: "1", name: "Katrin", salary: 11800000 })).toBe("");
  });
});

describe("colours", () => {
  it("of a ref differ by value", () => {
    const ref = f("employee_id", { kind: "ref", to: "hr_employee" });
    const tones = new Set(["a", "b", "c", "d", "e", "f"].map((v) => toneClassOf(ref, v)));
    expect(tones.size).toBeGreaterThan(1);
  });
});

describe("form sections", () => {
  it("put the record's name and its required refs before the hinted sections", () => {
    const m = model(
      [
        f("customer_id", { kind: "ref", to: "party", required: true }),
        f("name", { required: true }),
        f("amount", { kind: "money" }),
        f("seats", { kind: "number" }),
        f("a1"),
        f("a2"),
        f("a3"),
        f("a4"),
      ],
      { ui: { section: [{ label: "Billing", fields: ["amount", "seats"] }] } },
    );
    const s = sectionsOf(m, m.fields, { creating: true, row: null, filled: () => false });
    expect(s[0]!.fields.map((x) => x.name)).toContain("customer_id");
  });
});

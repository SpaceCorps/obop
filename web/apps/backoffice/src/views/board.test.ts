import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta } from "../api.ts";
import {
  boardLayout,
  columnTotals,
  draggableBy,
  dueTone,
  isFolded,
  isTerminal,
  mainMoneyOf,
  quickPlan,
  rangeEndOf,
  readFold,
  rowSums,
  showsSum,
  toggleFold,
} from "./board.ts";

const f = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const act = (name: string, from: string | null, to: string, field = "state"): Command => ({
  id: `m.${name}`,
  description: name,
  needs_approval: false,
  field,
  from,
  to,
});
const model = (fields: FieldMeta[], actions: Command[] = [], extra: Partial<ModelMeta> = {}) =>
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

const state = f("state", {
  kind: "select",
  required: true,
  options: ["draft", "submitted", "approved", "archived"],
});

describe("dragging", () => {
  it("is allowed between the values of a select or a yes/no, never a ref or a date", () => {
    expect(draggableBy(state)).toBe(true);
    expect(draggableBy(f("done", { kind: "bool" }))).toBe(true);
    expect(draggableBy(f("product_id", { kind: "ref", to: "product" }))).toBe(false);
    expect(draggableBy(f("due", { kind: "date" }))).toBe(false);
    expect(draggableBy(f("tier", { kind: "select", options: ["a"], computed: true }))).toBe(false);
    expect(draggableBy(undefined)).toBe(false);
  });
});

describe("finished columns", () => {
  it("know the words for an end", () => {
    const m = model([state]);
    expect(isTerminal(m, f("stage", { kind: "select", options: ["new", "won"] }), "won")).toBe(
      true,
    );
    expect(isTerminal(m, state, "draft")).toBe(false);
  });

  it("on a governed status: reached by an action and left by none the person may run", () => {
    const m = model(
      [state],
      [act("submit", "draft", "submitted"), act("approve", "submitted", "approved")],
    );
    expect(isTerminal(m, state, "approved")).toBe(true);
    expect(isTerminal(m, state, "submitted")).toBe(false);
    expect(isTerminal(m, state, "draft")).toBe(false);
    // An action from any state leaves every state.
    const reopen = model(
      [state],
      [act("approve", "submitted", "approved"), act("reset", null, "draft")],
    );
    expect(isTerminal(reopen, state, "approved")).toBe(false);
  });

  it("fold by default above ten cards, and the person's choice is kept either way", () => {
    let p = readFold(["lost"]);
    expect(isFolded(p, "lost", false)).toBe(true);
    expect(isFolded(p, "won", true)).toBe(true);
    p = toggleFold(p, "won", true);
    expect(isFolded(p, "won", true)).toBe(false);
    expect(p.open).toEqual(["won"]);
    p = toggleFold(p, "won", true);
    expect(isFolded(p, "won", true)).toBe(true);
    expect(p).toEqual({ fold: ["lost"], open: [] });
    p = toggleFold(p, "new", false);
    expect(p.fold).toEqual(["lost", "new"]);
    expect(readFold("nonsense")).toEqual({ fold: [], open: [] });
    expect(readFold({ fold: ["a", 3], open: "x" })).toEqual({ fold: ["a"], open: [] });
  });
});

describe("the amount of a column", () => {
  const money = (name: string, extra: Partial<FieldMeta> = {}) =>
    f(name, { kind: "money", ...extra });

  it("is the total, not a subtotal, a discount or a unit price", () => {
    const fields = [
      money("subtotal"),
      money("discount_total"),
      money("unit_price"),
      money("total"),
    ];
    expect(mainMoneyOf(model(fields), fields)?.name).toBe("total");
  });

  it("is never a person's own money, and follows the package's card_sum", () => {
    const salary = money("salary", { class: "financial", personal_data: true });
    expect(mainMoneyOf(model([salary]), [salary])).toBeUndefined();
    const bank = money("amount", { class: "financial" });
    expect(mainMoneyOf(model([bank]), [bank])?.name).toBe("amount");
    const fields = [money("total"), money("budget")];
    expect(mainMoneyOf(model(fields, [], { ui: { card_sum: "budget" } }), fields)?.name).toBe(
      "budget",
    );
    // A field withheld from the person is not chosen.
    expect(mainMoneyOf(model(fields), [fields[1]!])?.name).toBe("budget");
  });

  it("adds a group without a currency to the instance's, and keeps negative sums", () => {
    const amount = money("amount");
    const t = columnTotals(
      [
        { key: { state: "draft", currency: "EUR" }, count: 2, measures: { "sum:amount": 700 } },
        { key: { state: "draft", currency: null }, count: 1, measures: { "sum:amount": 300 } },
        { key: { state: "draft", currency: "SEK" }, count: 1, measures: { "sum:amount": 50 } },
        { key: { state: "approved", currency: "" }, count: 3, measures: { "sum:amount": -900 } },
      ],
      state,
      amount,
      "EUR",
    );
    expect([...t.get("draft")!.sums]).toEqual([
      ["EUR", 1000],
      ["SEK", 50],
    ]);
    expect(t.get("draft")!.count).toBe(4);
    expect([...t.get("approved")!.sums]).toEqual([["EUR", -900]]);
    expect(showsSum(t.get("approved")!.sums)).toBe(true);
    expect(showsSum(new Map([["EUR", 0]]))).toBe(false);
    expect(showsSum(undefined)).toBe(false);
  });

  it("from records in the browser: by each record's currency, else the instance's", () => {
    const sums = rowSums(
      [
        { id: "1", amount: 100, currency: "SEK" },
        { id: "2", amount: 50 },
        { id: "3", amount: null },
        { id: "4", amount: -20, currency: "" },
      ],
      money("amount"),
      "EUR",
    );
    expect([...sums]).toEqual([
      ["SEK", 100],
      ["EUR", 30],
    ]);
  });
});

describe("cards", () => {
  it("leave out the field the board is grouped by and show the status otherwise", () => {
    const fields = [
      f("title", { required: true }),
      state,
      f("priority", { kind: "select", options: ["low", "high"] }),
      f("customer_id", { kind: "ref", to: "party", required: true }),
    ];
    const m = model(fields, [act("submit", "draft", "submitted")]);
    const byState = boardLayout(m, fields, state);
    expect(byState.title).toEqual(["title"]);
    expect(byState.state).toBeUndefined();
    expect(byState.badges).toEqual(["priority"]);
    expect(byState.subtitle).toEqual(["customer_id"]);
    const byPriority = boardLayout(m, fields, fields[2]);
    expect(byPriority.state).toBe("state");
    expect(byPriority.badges).toEqual([]);
    // A field withheld from the person is not on the card.
    expect(boardLayout(m, [fields[0]!, state], state).subtitle).toEqual([]);
  });

  it("title a record known by a ref and a date by the ref, with the date range as the chip", () => {
    const fields = [
      f("employee_id", { kind: "ref", to: "hr_employee", required: true }),
      f("start_on", { kind: "date", required: true }),
      f("end_on", { kind: "date", required: true }),
    ];
    const l = boardLayout(model(fields), fields);
    expect(l.titleTemplate).toBe("{employee_id}");
    expect(l.date).toBe("start_on");
    expect(l.dateEnd).toBe("end_on");
  });

  it("find the end of a range", () => {
    const fields = [
      f("start_on", { kind: "date" }),
      f("end_on", { kind: "date" }),
      f("date_from", { kind: "date" }),
      f("date_to", { kind: "date" }),
      f("due_on", { kind: "date" }),
    ];
    expect(rangeEndOf(fields, fields[0])?.name).toBe("end_on");
    expect(rangeEndOf(fields, fields[2])?.name).toBe("date_to");
    expect(rangeEndOf(fields, fields[4])).toBeUndefined();
  });

  it("read a due date as late once it has passed and as close within a week", () => {
    const due = f("due_on", { kind: "date" });
    expect(dueTone(due, "2026-11-10", "2026-11-15")).toBe("bad");
    expect(dueTone(due, "2026-11-20", "2026-11-15")).toBe("warn");
    expect(dueTone(due, "2026-12-20", "2026-11-15")).toBeNull();
    expect(dueTone(due, "2026-11-10", "2026-11-15", true)).toBeNull();
    expect(dueTone(f("hired_on", { kind: "date" }), "2026-11-10", "2026-11-15")).toBeNull();
    const soon = f("renew_on", { kind: "date", ui: { warn_before_days: 30 } });
    expect(dueTone(soon, "2026-12-10", "2026-11-15")).toBe("warn");
  });
});

describe("quick create", () => {
  it("asks for the name and every required field no default fills", () => {
    const fields = [
      f("name", { required: true }),
      f("customer_id", { kind: "ref", to: "party", required: true }),
      f("scheduled_on", { kind: "date", required: true }),
      f("notes", { kind: "longtext" }),
      f("number", { sequence: true }),
      state,
    ];
    const m = model(fields, [act("start", "draft", "submitted")]);
    const plan = quickPlan(m, fields, { scheduled_on: "2026-11-15" });
    expect(plan.fields.map((x) => x.name)).toEqual(["name", "customer_id"]);
    expect(plan.full).toBe(false);
  });

  it("opens the full form for a picture or for more than a quick form takes", () => {
    const pic = [f("name", { required: true }), f("photo", { kind: "image", required: true })];
    expect(quickPlan(model(pic), pic, {}).full).toBe(true);
    const many = [
      f("name", { required: true }),
      ...["a", "b", "c", "d"].map((n) => f(n, { required: true })),
    ];
    expect(quickPlan(model(many), many, {}).full).toBe(true);
  });

  it("asks whom it is for when there is no name, and follows the package's quick_create", () => {
    const fields = [f("number", { sequence: true }), f("party_id", { kind: "ref", to: "party" })];
    expect(quickPlan(model(fields), fields, {}).fields.map((x) => x.name)).toEqual(["party_id"]);
    const hinted = [f("title", { required: true }), f("amount", { kind: "money" }), f("x")];
    const plan = quickPlan(
      model(hinted, [], { ui: { quick_create: ["title", "amount"] } }),
      hinted,
      {},
    );
    expect(plan.fields.map((x) => x.name)).toEqual(["title", "amount"]);
    expect(plan.full).toBe(false);
  });
});

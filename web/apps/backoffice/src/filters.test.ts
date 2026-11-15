// filters.ts: the filters the control panel generates and the fields its search offers.
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";
import {
  dateFilters,
  isOn,
  openStates,
  optionFields,
  parseWhere,
  quickFilters,
  quickLabel,
  rangeDays,
  rangeFields,
  sameCond,
  searchTargets,
  todayIn,
  toggleQuick,
  type QuickFilter,
} from "./filters.ts";
import type { Cond } from "./query.ts";

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en");
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
const act = (id: string, from: string | null, to: string): Command => ({
  id: `ticket.${id}`,
  description: "",
  needs_approval: false,
  field: "state",
  from,
  to,
});

const ticket = model(
  "ticket",
  [
    f("subject", "text", { required: true }),
    f("number", "text", { sequence: true }),
    f("state", "select", {
      options: ["new", "in_progress", "resolved", "closed", "cancelled"],
      governed: true,
    }),
    f("priority", "select", { options: ["low", "high"] }),
    f("customer_id", "ref", { to: "party", required: true }),
    f("assignee", "text"),
    f("deadline", "date"),
    f("opened_on", "date", { required: true }),
    f("health_note", "longtext", { class: "special_category" }),
    f("iban", "text", { class: "financial" }),
    f("external_id", "text"),
    f("active", "bool"),
  ],
  {
    actions: [
      act("take", "new", "in_progress"),
      act("resolve", "in_progress", "resolved"),
      act("close", "resolved", "closed"),
      act("reopen", "closed", "in_progress"),
      act("cancel", null, "cancelled"),
    ],
  },
);
const ctx = { user: "ada", today: "2026-11-15", firstDay: 1 };

describe("search targets", () => {
  it("offers at most five: title, number, refs, selects; never private or id-like fields", () => {
    const names = searchTargets(ticket, ticket.fields).map((x) => x.name);
    expect(names).toEqual(["subject", "number", "customer_id", "state", "priority"]);
    expect(names).not.toContain("iban");
    expect(names).not.toContain("health_note");
    expect(names).not.toContain("external_id");
  });
  it("only fields the person sees", () => {
    const seen = ticket.fields.filter((x) => x.name !== "customer_id");
    expect(searchTargets(ticket, seen).map((x) => x.name)).not.toContain("customer_id");
  });
});

describe("open states", () => {
  it("are the states with a move forward, never a bad end", () => {
    expect(openStates(ticket)).toEqual({
      field: ticket.fields.find((x) => x.name === "state"),
      states: ["new", "in_progress", "resolved"],
    });
  });
  it("need actions on the status", () => {
    expect(openStates(model("x", ticket.fields))).toBeNull();
  });
});

describe("quick filters", () => {
  const quick = quickFilters(ticket, ticket.fields, ctx);
  const byKey = (k: string) => quick.find((q) => q.key === k)!;
  it("generates Open, Overdue, Mine and Archived", () => {
    expect(quick.map((q) => q.label)).toEqual(["Open", "Overdue", "Mine", "Archived"]);
    expect(byKey("open").conds).toEqual([
      { op: "is", f: "state", v: ["new", "in_progress", "resolved"] },
    ]);
    expect(byKey("overdue").conds).toEqual([
      { op: "range", f: "deadline", max: "2026-11-14" },
      { op: "is", f: "state", v: ["new", "in_progress", "resolved"] },
    ]);
    expect(byKey("mine").conds).toEqual([{ op: "is", f: "assignee", v: ["ada"] }]);
    expect(byKey("archived").conds).toEqual([{ op: "bool", f: "active", v: false }]);
  });
  it("Mine follows the owner the server knows, and needs a user", () => {
    const own = model("doc", [f("owner", "text"), f("assignee", "text")], {
      access: { own: "owner" },
    });
    expect(quickFilters(own, own.fields, ctx).find((q) => q.kind === "mine")?.conds).toEqual([
      { op: "is", f: "owner", v: ["ada"] },
    ]);
    expect(quickFilters(own, own.fields, { today: ctx.today }).some((q) => q.kind === "mine")).toBe(
      false,
    );
  });
  it("an expiry date says Expired", () => {
    const lic = model("licence", [f("expires_on", "date")]);
    expect(quickFilters(lic, lic.fields, ctx).map((q) => q.label)).toEqual(["Expired"]);
  });
  it("saved filters from the package come first, and one the browser cannot read is left out", () => {
    const m = {
      ...ticket,
      ui: {
        filter: [
          {
            label: "My open",
            where: 'assignee == $user && (state == "new" || state == "resolved")',
          },
          { label: "Odd", where: "sum(lines.qty) > 3" },
        ],
      },
    };
    const q = quickFilters(m, m.fields, ctx);
    expect(q[0]).toMatchObject({
      label: "My open",
      conds: [
        { op: "is", f: "assignee", v: ["ada"] },
        { op: "is", f: "state", v: ["new", "resolved"] },
      ],
    });
    expect(q.some((x) => x.label === "Odd")).toBe(false);
  });
});

describe("toggling", () => {
  const quick = quickFilters(ticket, ticket.fields, ctx);
  const open = quick.find((q) => q.key === "open")!;
  it("on replaces a clashing condition on the same field, off removes it", () => {
    const before: Cond[] = [
      { op: "is", f: "state", v: ["closed"] },
      { op: "search", v: "drill" },
    ];
    const on = toggleQuick(before, open);
    expect(on).toEqual([{ op: "search", v: "drill" }, open.conds[0]]);
    expect(isOn(on, open)).toBe(true);
    expect(toggleQuick(on, open)).toEqual([{ op: "search", v: "drill" }]);
  });
  it("matches values in any order", () => {
    expect(
      sameCond(
        { op: "is", f: "state", v: ["resolved", "new", "in_progress"] },
        open.conds[0] as Cond,
      ),
    ).toBe(true);
  });
  it("names a chip after its filter", () => {
    const dates = dateFilters(
      ticket.fields.find((x) => x.name === "opened_on")!,
      ctx,
    );
    const all: QuickFilter[] = [...quick, ...dates];
    expect(quickLabel(open.conds[0]!, all)).toBe("Open");
    expect(quickLabel({ op: "range", f: "deadline", max: "2026-11-14" }, all)).toBe("Overdue");
    expect(quickLabel(dates[2]!.conds[0]!, all)).toBe("Opened on: This month");
    expect(quickLabel({ op: "search", v: "x" }, all)).toBeUndefined();
  });
});

describe("days", () => {
  it("ranges around a day", () => {
    expect(rangeDays("today", "2026-11-15")).toEqual({ min: "2026-11-15", max: "2026-11-15" });
    // 15 Nov 2026 is a Sunday: the week from Monday 9 to Sunday 15, or Sunday 15 to Saturday 21.
    expect(rangeDays("week", "2026-11-15", 1)).toEqual({ min: "2026-11-09", max: "2026-11-15" });
    expect(rangeDays("week", "2026-11-15", 7)).toEqual({ min: "2026-11-15", max: "2026-11-21" });
    expect(rangeDays("month", "2026-02-10")).toEqual({ min: "2026-02-01", max: "2026-02-28" });
    expect(rangeDays("last30", "2026-11-15")).toEqual({ min: "2026-10-17", max: "2026-11-15" });
    expect(rangeDays("year", "2026-11-15")).toEqual({ min: "2026-01-01", max: "2026-12-31" });
  });
  it("today in a zone", () => {
    const late = new Date("2026-11-15T23:30:00Z");
    expect(todayIn("Europe/Berlin", late)).toBe("2026-11-16");
    expect(todayIn("UTC", late)).toBe("2026-11-15");
  });
  it("ranges go on the required date first, at most two fields", () => {
    expect(rangeFields(ticket.fields).map((x) => x.name)).toEqual(["opened_on", "deadline"]);
  });
});

describe("option fields", () => {
  it("the status first", () => {
    const m = model("x", [f("kind", "select", { options: ["a"] }), ...ticket.fields]);
    expect(optionFields(m, m.fields).map((x) => x.name)).toEqual(["state", "kind", "priority"]);
  });
});

describe("where", () => {
  const w = (s: string) => parseWhere(s, ticket, ctx);
  it("reads comparisons, is_set and yes/no fields", () => {
    expect(w('state != "closed" and active')).toEqual([
      { op: "isnot", f: "state", v: ["closed"] },
      { op: "bool", f: "active", v: true },
    ]);
    expect(w("deadline < today && !is_set(customer_id)")).toEqual([
      { op: "range", f: "deadline", max: "2026-11-14" },
      { op: "empty", f: "customer_id" },
    ]);
    expect(w("opened_on >= today-7")).toEqual([{ op: "range", f: "opened_on", min: "2026-11-08" }]);
    expect(w("is_set(priority)")).toEqual([{ op: "isnot", f: "priority", v: [""] }]);
    expect(w("!active")).toEqual([{ op: "bool", f: "active", v: false }]);
  });
  it("refuses what it cannot express", () => {
    expect(w("state == 'new' || priority == 'high'")).toBeNull();
    expect(w("(state == 'new' || priority == 'high')")).toBeNull();
    expect(w("nope == 1")).toBeNull();
    expect(w("assignee == $user")).toEqual([{ op: "is", f: "assignee", v: ["ada"] }]);
    expect(parseWhere("assignee == $user", ticket, { today: ctx.today })).toBeNull();
    expect(w("")).toBeNull();
  });
});

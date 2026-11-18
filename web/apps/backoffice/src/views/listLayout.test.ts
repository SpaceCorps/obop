// The list view's layout decisions (lane L2): columns, the first column, phone cards, deadlines,
// the default order, the sort cycle, and the totals of query.ts that the footer shows.
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { fmt, provideCatalogue, setInstanceDefaults, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "../api.ts";
import { formatSumRows, sortRows, type Lookup } from "../query.ts";
import { toServer } from "../serverQuery.ts";
import {
  cardMeta,
  cardRoles,
  chooseColumns,
  droppable,
  dueLevel,
  columnWidth,
  editableField,
  effectiveOrder,
  fitColumns,
  firstColumnMode,
  isDueLike,
  isMasterData,
  listOrder,
  listSummable,
  nextSort,
  trimColumns,
  canSortBy,
} from "./listLayout.ts";

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
const act = (id: string, field: string, from: string, to: string): Command => ({
  id,
  description: "",
  needs_approval: false,
  field,
  from,
  to,
});
const row = (o: Record<string, unknown>): Row => ({ id: "r1", ...o }) as Row;
const names = (fs: FieldMeta[]) => fs.map((x) => x.name);

// An expense: a number, a description, the claimant, a date, an amount (financial) and a status.
const state = f("state", "select", {
  options: ["draft", "submitted", "approved", "paid", "refused"],
  governed: true,
});
const expense = model(
  "expenses_expense",
  [
    f("number", "text", { sequence: true }),
    f("description", "text", { required: true }),
    f("employee_id", "ref", { to: "hr_employee", required: true }),
    f("spent_on", "date", { required: true }),
    f("category", "select", { options: ["travel", "meals"] }),
    f("amount_minor", "money", { class: "financial" }),
    f("currency", "text"),
    f("notes", "longtext"),
    state,
  ],
  { actions: [act("expenses_expense.submit", "state", "draft", "submitted")] },
);

describe("the columns", () => {
  it("rank by role and keep the status last; a narrow list keeps title, money and status", () => {
    const wide = chooseColumns(expense, expense.fields, 1400, [], null);
    expect(names(wide)[0]).toBe("description");
    expect(names(wide).at(-1)).toBe("state");
    expect(names(wide)).toContain("amount_minor");
    expect(names(wide)).not.toContain("currency");
    expect(names(wide)).not.toContain("notes");
    const narrow = chooseColumns(expense, expense.fields, 400, [], null);
    expect(names(narrow)).toEqual(["description", "amount_minor", "state"]);
  });

  it("are the person's own choice when there is one, without names that are gone", () => {
    const own = chooseColumns(expense, expense.fields, 400, [], ["state", "gone", "spent_on"]);
    expect(names(own)).toEqual(["state", "spent_on"]);
    // Nothing left of the choice: the ranking again.
    expect(names(chooseColumns(expense, expense.fields, 400, [], ["gone"]))[0]).toBe("description");
  });

  it("read ui.list when the server sends it", () => {
    const hinted = { ...expense, ui: { list: ["number", "spent_on", "amount_minor"] } };
    expect(names(chooseColumns(hinted, hinted.fields, 1400, [], null))).toEqual([
      "number",
      "spent_on",
      "amount_minor",
      "state",
    ]);
  });
});

describe("the first column", () => {
  it("draws the whole title, unless every part of the title has a column of its own", () => {
    const account = model("accounting_account", [f("code", "text"), f("name", "text")]);
    expect(firstColumnMode(account, account.fields)).toBe("value");
    expect(firstColumnMode(account, [account.fields[0]!])).toBe("title");
    expect(firstColumnMode(expense, chooseColumns(expense, expense.fields, 1400, [], null))).toBe(
      "title",
    );
    // Columns the person put first that are not the title: the value, never a link.
    expect(firstColumnMode(expense, [state])).toBe("value");
  });
});

describe("a phone card", () => {
  it("has the title, the status, the amount (also a financial one) and the number, ref and date", () => {
    const r = cardRoles(expense, expense.fields, []);
    expect(r.title?.name).toBe("description");
    expect(r.state?.name).toBe("state");
    expect(r.money?.name).toBe("amount_minor");
    const full = row({ number: "EXP-1", employee_id: "e1", spent_on: "2026-11-14", category: "x" });
    expect(names(cardMeta(r, full))).toEqual(["number", "employee_id", "spent_on"]);
    // What a record has not filled in is skipped, and the next field takes its place.
    expect(names(cardMeta(r, row({ spent_on: "2026-11-14", category: "travel" })))).toEqual([
      "spent_on",
      "category",
    ]);
  });

  it("takes the first ref and date the record has filled in", () => {
    const req = model("sign_request", [
      f("title", "text", { required: true }),
      f("template_id", "ref", { to: "sign_template" }),
      f("document_id", "ref", { to: "documents_document" }),
      f("sent_on", "date"),
    ]);
    const r = cardRoles(req, req.fields, []);
    expect(names(cardMeta(r, row({ document_id: "d1", sent_on: "2026-11-14" })))).toEqual([
      "document_id",
      "sent_on",
    ]);
  });

  it("prefers a due date and fills a card without refs or dates with the next fields", () => {
    const task = model("projects_task", [
      f("name", "text", { required: true }),
      f("start_on", "date"),
      f("due_on", "date"),
    ]);
    const both = row({ start_on: "2026-11-01", due_on: "2026-11-30" });
    expect(names(cardMeta(cardRoles(task, task.fields, []), both))).toEqual(["due_on"]);
    const party = model("party", [
      f("name", "text", { required: true }),
      f("email", "text"),
      f("city", "text"),
      f("photo", "image"),
    ]);
    const r = cardRoles(party, party.fields, []);
    expect(names(cardMeta(r, row({ email: "a@b.example", city: "Hamburg" })))).toEqual([
      "email",
      "city",
    ]);
    expect(r.image?.name).toBe("photo");
    expect(r.state).toBeUndefined();
    expect(r.money).toBeUndefined();
  });
});

describe("the totals and the fit of the columns", () => {
  it("do not add up a vendor's days, but do add up days of leave", () => {
    const vendor = model("purchase_vendor", [
      f("party_id", "ref", { to: "party", required: true }),
      f("payment_terms_days", "number"),
      f("active", "bool"),
    ]);
    expect(isMasterData(vendor)).toBe(true);
    expect(listSummable(vendor.fields[1]!, vendor)).toBe(false);
    const leave = model("hr_leave_request", [
      f("start_on", "date", { required: true }),
      f("days", "number"),
    ]);
    expect(isMasterData(leave)).toBe(false);
    expect(listSummable(leave.fields[1]!, leave)).toBe(true);
    const hinted = f("payment_terms_days", "number", { ui: { aggregate: "sum" } });
    expect(listSummable(hinted, vendor)).toBe(true);
  });

  it("drop the lowest-ranked columns first, never the title, the first amount or the status", () => {
    const cols = chooseColumns(expense, expense.fields, 1400, [], null);
    const kept = trimColumns(cols, 99, "state");
    expect(names(kept)).toEqual(["description", "amount_minor", "state"]);
    expect(droppable(cols, "state")).toBe(cols.length - 3);
    const one = trimColumns(cols, 1, "state");
    expect(one).toHaveLength(cols.length - 1);
    expect(names(one).at(-1)).toBe("state");
  });
});

describe("the widths of the columns", () => {
  const head = (x: FieldMeta) => x.name;
  it("follow the kind and a long head; a number or a code is a short title", () => {
    expect(columnWidth(f("spent_on", "date"), "Spent on")).toBe(136);
    expect(columnWidth(f("x", "number"), "Payment terms in days")).toBeGreaterThan(104);
    expect(columnWidth(f("x", "text"), "A".repeat(80))).toBe(260);
    expect(columnWidth(f("number", "text", { sequence: true }), "Number", true)).toBe(160);
    expect(columnWidth(f("description", "text"), "Description", true)).toBe(280);
  });

  it("fill the width exactly, the title taking the larger share, and drop what does not fit", () => {
    const ranked = chooseColumns(expense, expense.fields, 100_000, [], null);
    const wide = fitColumns(ranked, 2000, head, "state");
    expect(wide.cols).toHaveLength(ranked.length);
    // 52 px of selection column plus the columns make the whole width (give or take the rounding).
    const sum = wide.widths.reduce((a, b) => a + b, 52);
    expect(sum).toBeLessThanOrEqual(2000);
    expect(sum).toBeGreaterThan(2000 - wide.widths.length);
    const narrow = fitColumns(ranked, 700, head, "state");
    expect(names(narrow.cols)).toEqual(["description", "amount_minor", "state"]);
    expect(narrow.widths[0]).toBeGreaterThan(narrow.widths[1]!);
    // The person's own columns are never dropped (the table scrolls instead).
    expect(fitColumns(ranked, Infinity, head, "state").cols).toHaveLength(ranked.length);
  });

  it("beside a record panel, give the title the room and keep the status in view (R7)", () => {
    const states = { governed: true } as const;
    // Time off: the employee and the dates are the title, then the working days and the status.
    const leave = [
      f("employee_id", "ref", { to: "hr_employee" }),
      f("working_days", "number"),
      f("state", "select", { options: ["draft", "requested", "approved", "refused"], ...states }),
    ];
    const l = fitColumns(leave, 560, head, "state", leave[1]);
    expect(l.widths.reduce((a, b) => a + b, 52)).toBeLessThanOrEqual(560);
    // The title gets what the number and the badge do not need.
    expect(l.widths[0]).toBeGreaterThanOrEqual(240);
    expect(l.widths[2]).toBeLessThan(140);
    // Trips: a long title, the distance and five states; nothing more can be dropped.
    const trips = [
      f("driver", "text"),
      f("distance_m", "decimal"),
      f("state", "select", {
        options: ["recording", "ended", "in_review", "confirmed", "discarded"],
        ...states,
      }),
    ];
    const t = fitColumns(trips, 560, head, "state", trips[1]);
    expect(names(t.cols)).toEqual(["driver", "distance_m", "state"]);
    expect(t.widths.reduce((a, b) => a + b, 52)).toBeLessThanOrEqual(560);
  });
});

describe("cells that edit in place", () => {
  it("are not numbers the server gives, computed, read-only or files", () => {
    expect(editableField(f("title", "text"))).toBe(true);
    expect(editableField(f("number", "text", { sequence: true }))).toBe(false);
    expect(editableField(f("total", "money", { computed: true }))).toBe(false);
    expect(editableField(f("signed_ip", "text", { readonly: true }))).toBe(false);
    expect(editableField(f("image", "image"))).toBe(false);
  });
});

describe("deadlines", () => {
  const inv = model("invoice", [
    f("due_on", "date"),
    f("issued_on", "date"),
    f("state", "select", { options: ["draft", "posted", "paid", "cancelled"], governed: true }),
  ]);
  const due = inv.fields[0]!;
  it("are past in red and close in amber, by name or by ui.warn_before_days", () => {
    const today = "2026-11-15";
    expect(isDueLike(due)).toBe(true);
    expect(isDueLike(inv.fields[1]!)).toBe(false);
    expect(dueLevel(inv, due, row({ due_on: "2026-11-14", state: "draft" }), today)).toBe("past");
    expect(dueLevel(inv, due, row({ due_on: "2026-11-20", state: "draft" }), today)).toBe("soon");
    expect(dueLevel(inv, due, row({ due_on: "2026-12-20", state: "draft" }), today)).toBeNull();
    const hinted = f("issued_on", "date", { ui: { warn_before_days: 30 } });
    expect(dueLevel(inv, hinted, row({ issued_on: "2026-12-10" }), today)).toBe("soon");
  });

  it("do not matter once the record is settled (posted counts, paid and cancelled do not)", () => {
    const today = "2026-11-15";
    expect(dueLevel(inv, due, row({ due_on: "2026-01-01", state: "paid" }), today)).toBeNull();
    expect(dueLevel(inv, due, row({ due_on: "2026-01-01", state: "cancelled" }), today)).toBeNull();
  });

  it("keep warning on a record in force when the field asks for it (an expiring permit)", () => {
    const today = "2026-11-15";
    const permit = f("work_permit_expiry", "date", { ui: { warn_before_days: 60 } });
    const emp = model("hr_employee", [
      permit,
      f("state", "select", { options: ["onboarding", "active", "offboarded"], governed: true }),
    ]);
    const at = (d: string, s: string) => row({ work_permit_expiry: d, state: s });
    expect(dueLevel(emp, permit, at("2026-12-20", "active"), today)).toBe("soon");
    expect(dueLevel(emp, permit, at("2026-11-01", "active"), today)).toBe("past");
    expect(dueLevel(emp, permit, at("2026-11-01", "offboarded"), today)).toBeNull();
    // A finished record (done, paid) still ends the warning, hint or not.
    const hinted = f("due_on", "date", { ui: { warn_before_days: 7 } });
    const task = model("task", [
      hinted,
      f("state", "select", { options: ["todo", "done"], governed: true }),
    ]);
    expect(dueLevel(task, hinted, row({ due_on: "2026-11-01", state: "done" }), today)).toBeNull();
  });

  it("compare an instant with now", () => {
    const exp = f("expires_at", "datetime");
    const m = model("sign_link", [exp]);
    const now = "2026-11-15T12:00:00Z";
    expect(dueLevel(m, exp, row({ expires_at: "2026-11-15T11:00:00Z" }), "", now)).toBe("past");
    expect(dueLevel(m, exp, row({ expires_at: "2026-11-16T11:00:00Z" }), "", now)).toBe("soon");
  });
});

describe("the default order", () => {
  it("is the main date newest first, then the number, then the id", () => {
    expect(listOrder(expense, expense.fields)).toEqual([{ field: "spent_on", dir: "desc" }]);
    const numbered = model("invoice", [f("number", "text", { sequence: true }), state], {
      actions: [act("invoice.post", "state", "draft", "posted")],
    });
    expect(listOrder(numbered, numbered.fields)).toEqual([{ field: "number", dir: "desc" }]);
    const lead = model(
      "lead",
      [f("title", "text", { required: true }), f("stage", "select", { options: ["new", "won"] })],
      { actions: [act("lead.win", "stage", "new", "won")] },
    );
    expect(listOrder(lead, lead.fields)).toEqual([{ field: "id", dir: "desc" }]);
  });

  it("lists master data by its code, else by its name", () => {
    const account = model("accounting_account", [f("code", "text"), f("name", "text")]);
    expect(listOrder(account, account.fields)).toEqual([{ field: "code", dir: "asc" }]);
    const product = model("product", [f("name", "text", { required: true }), f("price", "money")]);
    expect(listOrder(product, product.fields)).toEqual([{ field: "name", dir: "asc" }]);
  });

  it("follows ui.order and leaves out what the person cannot sort by", () => {
    const hinted = { ...expense, ui: { order: "spent_on asc, number desc" } };
    expect(listOrder(hinted, hinted.fields)).toEqual([
      { field: "spent_on", dir: "asc" },
      { field: "number", dir: "desc" },
    ]);
    // spent_on withheld from this person: the order falls back to the id.
    const noDate = expense.fields.filter((x) => x.name !== "spent_on");
    expect(listOrder({ ...expense, ui: { order: "spent_on desc" } }, noDate)).toEqual([
      { field: "id", dir: "desc" },
    ]);
    // An agent reads the name masked (meta `masked`): the server would refuse that sort.
    const secret = model("hr_employee", [
      f("name", "text", { required: true, personal_data: true, masked: true }),
    ]);
    expect(listOrder(secret, secret.fields)).toEqual([{ field: "id", dir: "desc" }]);
  });

  it("gives way to the person's sort, and a click on the default column turns it round", () => {
    const order = listOrder(expense, expense.fields);
    expect(effectiveOrder(expense, expense.fields, { f: "number", dir: "asc" })).toEqual([
      { field: "number", dir: "asc" },
    ]);
    expect(effectiveOrder(expense, expense.fields, undefined)).toEqual(order);
    expect(nextSort(undefined, "spent_on", order)).toEqual({ f: "spent_on", dir: "asc" });
    expect(nextSort({ f: "spent_on", dir: "asc" }, "spent_on", order)).toEqual({
      f: "spent_on",
      dir: "desc",
    });
    expect(nextSort({ f: "spent_on", dir: "desc" }, "spent_on", order)).toBeUndefined();
    expect(nextSort(undefined, "number", order)).toEqual({ f: "number", dir: "asc" });
  });
});

describe("the order the server is asked for (serverQuery.ts)", () => {
  it("is the default order without a sort of the person's own, and theirs with one", () => {
    const order = listOrder(expense, expense.fields);
    const none = toServer({ facets: [], group: [] }, expense.fields, order);
    expect(none.order).toEqual([{ field: "spent_on", dir: "desc" }]);
    const own = toServer(
      { facets: [], group: [], sort: { f: "number", dir: "asc" } },
      expense.fields,
      order,
    );
    expect(own.order).toEqual([{ field: "number", dir: "asc" }]);
  });
});

describe("the totals and the order of rows in the browser (query.ts)", () => {
  const amount = f("amount_minor", "money");
  const lk: Lookup = { refText: (_m, id) => id, refIds: () => [] };

  it("count a record without a currency in the instance's currency (B7)", () => {
    const rows = [
      row({ id: "a", amount_minor: 120000, currency: "EUR" }),
      row({ id: "b", amount_minor: 4545652 }),
      row({ id: "c", amount_minor: 100, currency: "" }),
    ];
    expect(formatSumRows(amount, rows)).toBe(fmt.money(120000 + 4545652 + 100, "EUR"));
  });

  it("keep currencies apart, through the currency the caller names", () => {
    const rows = [
      row({ id: "a", amount_minor: 100, currency: "EUR" }),
      row({ id: "b", amount_minor: 300, currency: "SEK" }),
      row({ id: "c", amount_minor: 200, journal: "se" }),
    ];
    const out = formatSumRows(amount, rows, (r) =>
      r.journal === "se" ? "SEK" : (r.currency as string),
    );
    expect(out).toContain(fmt.money(500, "SEK"));
    expect(out).toContain(fmt.money(100, "EUR"));
  });

  it("sort by the default order when the person chose none, with empty values last", () => {
    const date = f("spent_on", "date");
    const rows = [
      row({ id: "01", spent_on: "2026-01-02" }),
      row({ id: "02", spent_on: null }),
      row({ id: "03", spent_on: "2026-03-01" }),
      row({ id: "04", spent_on: "2026-03-01" }),
    ];
    const order = [
      { field: "spent_on", dir: "desc" as const },
      { field: "id", dir: "desc" as const },
    ];
    expect(sortRows(rows, undefined, [date], lk, order).map((r) => r.id)).toEqual([
      "04",
      "03",
      "01",
      "02",
    ]);
    // The person's sort wins over the default.
    expect(
      sortRows(rows, { f: "spent_on", dir: "asc" }, [date], lk, order).map((r) => r.id),
    ).toEqual(["01", "03", "04", "02"]);
    expect(sortRows(rows, undefined, [date], lk)).toBe(rows);
  });
});

describe("what a list may be sorted by (#89, R13)", () => {
  const trip = model(
    "mileage_trip",
    [
      f("driver", "text", { required: true }),
      f("started_at", "datetime", { class: "confidential_hr", personal_data: true }),
      f("photo", "image"),
      f("receipt", "file"),
    ],
    { ui: { order: "started_at desc" } },
  );
  it("never by a picture or a file, a masked value or a secret", () => {
    const by = (name: string) => trip.fields.find((x) => x.name === name)!;
    expect(canSortBy(by("photo"))).toBe(false);
    expect(canSortBy(by("receipt"))).toBe(false);
    expect(canSortBy(by("started_at"))).toBe(true);
    expect(canSortBy({ ...by("started_at"), masked: true })).toBe(false);
    expect(canSortBy(f("pin", "text", { class: "secret" }))).toBe(false);
  });
  it("the trip queue is in trip order for a person, who reads personal data in clear", () => {
    expect(listOrder(trip, trip.fields)).toEqual([{ field: "started_at", dir: "desc" }]);
    const agent = trip.fields.map((x) => (x.personal_data ? { ...x, masked: true } : x));
    expect(listOrder(trip, agent)).toEqual([{ field: "id", dir: "desc" }]);
  });
  it("a saved or linked sort on a picture is ignored, on the server too", () => {
    const sort = { f: "photo", dir: "asc" as const };
    const fallback = listOrder(trip, trip.fields);
    expect(effectiveOrder(trip, trip.fields, sort)).toEqual(fallback);
    expect(toServer({ facets: [], group: [], sort }, trip.fields, fallback).order).toEqual(
      fallback,
    );
  });
});

// presentation.ts: each function with a D95 hint and without one (the server sends none today).
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { fmt, provideCatalogue, setInstanceDefaults, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, ModuleMeta, Row } from "./api.ts";
import {
  cardLayout,
  childrenOf,
  currencyOf,
  defaultOrder,
  defaultView,
  displayOf,
  fillTemplate,
  homeOf,
  indexModels,
  isIdLike,
  listColumns,
  menuOf,
  newDefaults,
  nextActions,
  pickerSecondLine,
  searchFieldsOf,
  subtitleOf,
  summable,
  titleOf,
  toneClassOf,
  toneOf,
} from "./presentation.ts";

/** This year, so that short dates read "14 Nov" without a year. */
const Y = new Date().getFullYear();
/** A short date as the active region writes it ("14 Nov" or "Nov 14"). */
const short = (d: string, year = false) =>
  fmt.date(d, { day: "numeric", month: "short", ...(year ? { year: "numeric" } : {}) });

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
const act = (id: string, field: string, from: string | null, to: string, more = {}): Command => ({
  id,
  description: "",
  needs_approval: false,
  field,
  from,
  to,
  ...more,
});
const row = (v: Record<string, unknown>): Row => ({
  id: "01a1dc5a-4dc3-705e-acb1-89b8d8849d9c",
  ...v,
});

const party = model("party", [f("name", "text", { required: true })]);
const journal = model("accounting_journal", [
  f("code", "text", { required: true }),
  f("name", "text", { required: true }),
  f("currency", "text"),
]);
const move = model(
  "accounting_move",
  [
    f("journal_id", "ref", { to: "accounting_journal", required: true }),
    f("date", "date", { required: true }),
    f("number", "text", { sequence: true, gapless: true }),
    f("partner_id", "ref", { to: "party" }),
    f("narration", "longtext"),
    f("total_debit", "money", { computed: true, class: "financial" }),
    f("state", "select", {
      required: true,
      governed: true,
      options: ["draft", "posted", "cancelled", "reversed"],
    }),
  ],
  {
    actions: [
      act("accounting_move.post", "state", "draft", "posted"),
      act("accounting_move.cancel", "state", "draft", "cancelled"),
      act("accounting_move.reverse", "state", "posted", "reversed"),
    ],
  },
);
const line = model("accounting_move_line", [
  f("move_id", "ref", { to: "accounting_move", required: true }),
  f("account_id", "ref", { to: "accounting_account", required: true }),
  f("debit", "money"),
]);
const account = model("accounting_account", [
  f("code", "text", { required: true }),
  f("name", "text", { required: true }),
  f("parent_id", "ref", { to: "accounting_account" }),
]);
const ticket = model(
  "helpdesk_ticket",
  [
    f("number", "text", { sequence: true }),
    f("subject", "text", { required: true }),
    f("description", "longtext"),
    f("priority", "select", { options: ["low", "normal", "high", "urgent"] }),
    f("category", "select", { options: ["question", "incident"] }),
    f("sla_id", "ref", { to: "helpdesk_sla" }),
    f("due_on", "date"),
    f("state", "select", {
      required: true,
      governed: true,
      options: ["open", "in_progress", "resolved", "closed"],
    }),
  ],
  {
    actions: [
      act("helpdesk_ticket.take", "state", "open", "in_progress"),
      act("helpdesk_ticket.resolve", "state", "in_progress", "resolved"),
    ],
  },
);
const sla = model("helpdesk_sla", [f("name", "text", { required: true }), f("hours", "number")]);
const trip = model("mileage_trip", [
  f("client_trip_id", "text", { required: true, class: "internal" }),
  f("started_at", "datetime", { required: true }),
  f("start_lat", "decimal", { class: "confidential_hr" }),
  f("distance_m", "decimal"),
]);
const subscription = model("subscriptions_subscription", [
  f("customer_id", "ref", { to: "party", required: true }),
  f("plan_id", "ref", { to: "subscriptions_plan", required: true }),
  f("started_on", "date"),
  f("recurring_amount", "money", { required: true }),
  f("currency", "text"),
]);

const labels: Record<string, string> = {
  "accounting_journal:j1": "Bank",
  "party:p1": "Baumann Bau GmbH",
  "subscriptions_plan:pl": "Pro",
};
const refText = (m: string, id: string) => labels[`${m}:${id}`] ?? "";

describe("titleOf", () => {
  it("uses the template first, the first alternative whose fields are set", () => {
    const hinted = { ...move, ui: { title: "{number} | Draft · {journal_id} · {date}" } };
    const draft = row({ journal_id: "j1", date: `${Y}-11-14`, state: "draft" });
    expect(titleOf(hinted, draft, { refText })).toBe(`Draft · Bank · ${short(`${Y}-11-14`)}`);
    expect(titleOf(hinted, { ...draft, number: "JE/2026/00042" }, { refText })).toBe(
      "JE/2026/00042",
    );
  });
  it("joins code and name", () => {
    expect(titleOf(account, row({ code: "1400", name: "Trade receivables" }))).toBe(
      "1400 Trade receivables",
    );
  });
  it("says Draft with the main ref and date for a draft without its number", () => {
    const t = titleOf(move, row({ journal_id: "j1", date: `${Y}-11-14`, state: "draft" }), {
      refText,
    });
    expect(t).toBe(`Draft · Bank · ${short(`${Y}-11-14`)}`);
  });
  it("names a record by its main ref and date, never by its currency", () => {
    const t = titleOf(
      subscription,
      row({ customer_id: "p1", plan_id: "pl", started_on: "2025-01-02", currency: "EUR" }),
      { refText },
    );
    expect(t).toBe(`Baumann Bau GmbH · ${short("2025-01-02", true)}`);
    expect(t).not.toContain("EUR");
  });
  it("never shows an id-like text", () => {
    const t = titleOf(
      trip,
      row({ client_trip_id: "6f0c4b1e-uuid", started_at: `${Y}-11-14T08:00:00Z` }),
    );
    expect(t).not.toContain("6f0c4b1e");
    expect(t).not.toContain("01a1dc5a");
  });
  it("falls back to the model's name, not the id", () => {
    expect(titleOf(trip, row({}), { moduleId: "mileage" })).toBe("Trip");
  });
  it("follows parent_id into a path", () => {
    const rows: Record<string, Row> = {
      a: { id: "a", code: "1", name: "Assets" },
      b: { id: "b", code: "14", name: "Receivables", parent_id: "a" },
    };
    const rowOf = (_m: string, id: string) => rows[id];
    expect(
      titleOf(account, { id: "c", code: "1400", name: "Trade", parent_id: "b" }, { rowOf }),
    ).toBe("1 Assets / 14 Receivables / 1400 Trade");
  });
});

describe("second lines and search", () => {
  it("subtitle shows the main ref and date not used by the title", () => {
    const r = row({ subject: "Printer", sla_id: "", due_on: `${Y}-11-20` });
    expect(subtitleOf(ticket, r)).toBe(short(`${Y}-11-20`));
  });
  it("the subtitle template wins", () => {
    const m = { ...ticket, ui: { subtitle: "#{number}" } };
    expect(subtitleOf(m, row({ number: "T-7" }))).toBe("#T-7");
  });
  it("picker second line prefers a code", () => {
    const product = model("product", [
      f("name", "text", { required: true }),
      f("sku", "text"),
      f("list_price", "money"),
    ]);
    expect(pickerSecondLine(product, row({ name: "Jigsaw", sku: "JS-1", list_price: 100 }))).toBe(
      "JS-1",
    );
    expect(pickerSecondLine(product, row({ name: "Jigsaw", list_price: 12345 }))).toContain("123");
  });
  it("search fields: the title, the number and codes, no private or id-like field", () => {
    expect(searchFieldsOf(ticket)).toEqual(["subject", "number"]);
    expect(searchFieldsOf(account)).toEqual(["code", "name"]);
    expect(searchFieldsOf(trip)).not.toContain("client_trip_id");
  });
  it("fills templates with paths", () => {
    const rowOf = (m: string, id: string) =>
      m === "accounting_journal" && id === "j1" ? { id, currency: "SEK" } : undefined;
    expect(fillTemplate(move, row({ journal_id: "j1" }), "{journal_id.currency}", { rowOf })).toBe(
      "SEK",
    );
  });
  it("knows id-like names", () => {
    expect(isIdLike(f("client_trip_id", "text"))).toBe(true);
    expect(isIdLike(f("tax_id", "text"))).toBe(true);
    expect(isIdLike(f("party_id", "ref"))).toBe(false);
  });
});

describe("listColumns", () => {
  it("ranks title, ref, date, money and pins the state last", () => {
    const cols = listColumns(move, 1440).map((x) => x.name);
    expect(cols[0]).toBe("number");
    expect(cols[1]).toBe("journal_id");
    expect(cols.at(-1)).toBe("state");
    expect(cols).toContain("total_debit");
    expect(cols).not.toContain("narration");
  });
  it("drops low ranks as the width shrinks, keeping title, money and state", () => {
    const narrow = listColumns(ticket, 360).map((x) => x.name);
    expect(narrow[0]).toBe("subject");
    expect(narrow.at(-1)).toBe("state");
    expect(narrow.length).toBeLessThan(listColumns(ticket, 1440).length);
  });
  it("leaves out private, id-like and coordinate fields", () => {
    const cols = listColumns(trip, 1440).map((x) => x.name);
    expect(cols).not.toContain("client_trip_id");
    expect(cols).not.toContain("start_lat");
  });
  it("leaves out columns empty on every loaded row", () => {
    const cols = listColumns(ticket, 1440, { rows: [row({ subject: "a", state: "open" })] });
    expect(cols.map((x) => x.name)).toEqual(["subject", "state"]);
  });
  it("follows the list hint", () => {
    const m = { ...ticket, ui: { list: ["number", "subject"] } };
    expect(listColumns(m, 1440).map((x) => x.name)).toEqual(["number", "subject", "state"]);
  });
});

describe("cardLayout", () => {
  it("picks roles and never a financial field", () => {
    const c = cardLayout(ticket);
    expect(c.title).toEqual(["subject"]);
    expect(c.overline).toBe("number");
    expect(c.state).toBe("state");
    expect(c.date).toBe("due_on");
    expect(c.badges).toEqual(["priority", "category"]);
    expect(cardLayout(move).money).toBeUndefined();
  });
  it("the card hint may name a financial field", () => {
    expect(cardLayout({ ...move, ui: { card: ["total_debit"] } }).money).toBe("total_debit");
  });
});

describe("summable and currency", () => {
  it("sums amounts and quantities, not prices, rates or levels", () => {
    expect(summable(f("amount", "money"))).toBe(true);
    expect(summable(f("list_price", "money"))).toBe(false);
    expect(summable(f("rate_bp", "number"))).toBe(false);
    expect(summable(f("reorder_level", "number"))).toBe(false);
    expect(summable(f("quantity", "number"))).toBe(true);
    expect(summable(f("odometer", "decimal"))).toBe(false);
    expect(summable(f("amount", "money"), undefined, true)).toBe(false);
    expect(summable(f("list_price", "money", { ui: { aggregate: "sum" } }))).toBe(true);
    expect(summable(f("amount", "money", { ui: { aggregate: "none" } }))).toBe(false);
  });
  it("takes the record's currency, then the path, then the instance's", () => {
    const m = {
      ...line,
      fields: line.fields.map((x) =>
        x.name === "debit" ? { ...x, ui: { currency_from: "move_id.journal_id.currency" } } : x,
      ),
    };
    const cached: Record<string, Row> = {
      "accounting_move:m1": { id: "m1", journal_id: "j1" },
      "accounting_journal:j1": { id: "j1", currency: "SEK" },
    };
    const rowOf = (mm: string, id: string) => cached[`${mm}:${id}`];
    const modelOf = (name: string) =>
      ({ accounting_move: move, accounting_journal: journal })[name];
    expect(currencyOf(row({ move_id: "m1" }), m, { rowOf, modelOf, instanceCurrency: "EUR" })).toBe(
      "SEK",
    );
    // Without the metadata of the move the second step is not followed.
    expect(currencyOf(row({ move_id: "m1" }), m, { rowOf, instanceCurrency: "EUR" })).toBe("EUR");
    expect(currencyOf(row({ currency: "USD" }), m)).toBe("USD");
    expect(currencyOf(row({ currency: "" }), m, { instanceCurrency: "EUR" })).toBe("EUR");
  });
});

describe("tones", () => {
  const state = ticket.fields.find((x) => x.name === "state")!;
  const category = ticket.fields.find((x) => x.name === "category")!;
  const priority = ticket.fields.find((x) => x.name === "priority")!;
  it("gives a status its meaning and a category a neutral tint", () => {
    expect(toneOf(state, "resolved", ticket)).toBe("ok");
    expect(toneOf(f("state", "select"), "refused")).toBe("bad");
    expect(toneOf(f("state", "select"), "submitted")).toBe("warn");
    expect(toneOf(category, "incident", ticket)).toBeNull();
    expect(toneOf(priority, "urgent", ticket)).toBe("bad");
    expect(toneClassOf(category, "incident", ticket)).toBe("t-2");
  });
  it("follows the tones hint", () => {
    expect(toneOf(f("kind", "select", { ui: { tones: { scrap: "bad" } } }), "scrap")).toBe("bad");
  });
});

describe("views and order", () => {
  it("a short status without a gapless number opens on the board", () => {
    expect(defaultView(ticket)).toBe("kanban");
    expect(defaultView(move)).toBe("list");
    expect(defaultView(ticket, ["list"])).toBe("list");
    expect(defaultView({ ...move, ui: { default_view: "gallery" } })).toBe("gallery");
  });
  it("a model planned by date and time opens on the calendar", () => {
    const appt = model("appointments_appointment", [
      f("date", "date", { required: true }),
      f("start_minute", "number"),
    ]);
    expect(displayOf(appt.fields[1]!)).toBe("time_of_day");
    expect(defaultView(appt)).toBe("calendar");
  });
  it("orders by the hint, the required date, the number or the id", () => {
    expect(defaultOrder({ ...move, ui: { order: "date desc, number desc" } })).toEqual([
      { field: "date", dir: "desc" },
      { field: "number", dir: "desc" },
    ]);
    expect(defaultOrder(move)).toEqual([{ field: "date", dir: "desc" }]);
    expect(defaultOrder(ticket)).toEqual([{ field: "number", dir: "desc" }]);
    expect(defaultOrder(sla)).toEqual([{ field: "id", dir: "desc" }]);
  });
});

describe("apps", () => {
  const accounting: ModuleMeta = {
    id: "accounting",
    name: "Accounting",
    models: [account, journal, move, line],
  };
  const helpdesk: ModuleMeta = { id: "helpdesk", name: "Helpdesk", models: [sla, ticket] };
  const index = indexModels([accounting, helpdesk, { id: "base", name: "Base", models: [party] }]);
  it("mirrors resolve_child", () => {
    expect(childrenOf(move, index)).toEqual([
      { model: "accounting_move_line", via: "move_id", shape: "lines" },
    ]);
    expect(childrenOf(ticket, index)).toEqual([]);
    const declared = { ...ticket, children: [{ model: "x", via: "y" }] };
    expect(childrenOf(declared, index)).toEqual([{ model: "x", via: "y" }]);
  });
  it("groups the menu: children hidden, targets of stateful models are configuration", () => {
    const menu = menuOf(helpdesk, index);
    expect(menu.main.map((m) => m.name)).toEqual(["helpdesk_ticket"]);
    expect(menu.configuration.map((m) => m.name)).toEqual(["helpdesk_sla"]);
    const acc = menuOf(accounting, index);
    expect(acc.hidden.map((m) => m.name)).toEqual(["accounting_move_line"]);
    expect(acc.configuration.map((m) => m.name)).toEqual(["accounting_journal"]);
  });
  it("opens an app on its home", () => {
    expect(homeOf(helpdesk, index)).toBe("helpdesk_ticket");
    expect(homeOf(accounting, index)).toBe("overview");
    expect(homeOf({ ...helpdesk, home: "helpdesk_sla" }, index)).toBe("helpdesk_sla");
  });
});

describe("nextActions", () => {
  it("offers only what is valid now, the forward move as primary, cancel apart", () => {
    const draft = nextActions(move, row({ state: "draft" }));
    expect(draft.primary?.id).toBe("accounting_move.post");
    expect(draft.danger.map((a) => a.id)).toEqual(["accounting_move.cancel"]);
    expect(draft.secondary).toEqual([]);
    const posted = nextActions(move, row({ state: "posted" }));
    expect(posted.primary).toBeUndefined();
    expect(posted.secondary.map((a) => a.id)).toEqual(["accounting_move.reverse"]);
  });
  it("shows one entry per group and follows primary and danger hints", () => {
    const sub = model(
      "subscriptions_subscription",
      [f("state", "select", { governed: true, options: ["trial", "active", "cancelled"] })],
      {
        actions: [
          act("s.activate", "state", "trial", "active"),
          act("s.cancel_trial", "state", "trial", "cancelled", { ui: { group: "cancel" } }),
          act("s.cancel", "state", "active", "cancelled", { ui: { group: "cancel" } }),
          act("s.note", "state", null, "active", { ui: { primary: true } }),
        ],
      },
    );
    const n = nextActions(sub, row({ state: "trial" }));
    expect(n.primary?.id).toBe("s.note");
    expect(n.danger.map((a) => a.id)).toEqual(["s.cancel_trial"]);
    expect(n.secondary.map((a) => a.id)).toEqual(["s.activate"]);
  });
});

describe("newDefaults", () => {
  it("merges heuristics, server defaults, facets and the caller's values", () => {
    const expense = model("expenses_expense", [
      f("description", "text", { required: true }),
      f("employee", "text", { default: "$user" }),
      f("spent_on", "date", { required: true }),
      f("due_on", "date", { default: "today+14" }),
      f("category", "select", { options: ["travel", "meals"] }),
      f("active", "bool"),
      f("currency", "text"),
      f("number", "text", { sequence: true }),
      f("state", "select", { governed: true, options: ["draft", "paid"] }),
    ]);
    const v = newDefaults(expense, {
      today: "2026-11-15",
      user: "ada",
      instanceCurrency: "EUR",
      facets: [
        { op: "is", f: "category", v: ["meals"] },
        { op: "is", f: "state", v: ["paid"] },
      ],
      route: { description: "Lunch" },
    });
    expect(v).toEqual({
      spent_on: "2026-11-15",
      due_on: "2026-11-29",
      employee: "ada",
      active: true,
      currency: "EUR",
      category: "meals",
      description: "Lunch",
    });
  });
});

describe("displayOf", () => {
  it("reads the hint, then the name", () => {
    expect(displayOf(f("x", "number", { ui: { display: "percent" } }))).toBe("percent");
    expect(displayOf(f("duration_minutes", "number"))).toBe("duration_minutes");
    expect(displayOf(f("rate_bp", "number"))).toBe("percent_bp");
    expect(displayOf(f("distance_m", "decimal"))).toBe("km_from_m");
    expect(displayOf(f("contact_email", "text"))).toBe("email");
    expect(displayOf(f("phone", "text"))).toBe("tel");
    expect(displayOf(f("name", "text"))).toBeUndefined();
  });
});

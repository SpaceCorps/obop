// recordLayout.ts: the sections of the form, the header's figures, locks and the full page.
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import { indexModels } from "./presentation.ts";
import {
  actionLabelFor,
  copyValues,
  createdRefs,
  deleteBlocked,
  keyFigures,
  lockOf,
  prefersFullPage,
  sectionsOf,
  stepsOf,
} from "./recordLayout.ts";

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
const row = (v: Record<string, unknown>): Row => ({ id: "r1", ...v }) as Row;
const filledIn = (r: Row | null) => (x: FieldMeta) =>
  !!r && r[x.name] !== undefined && r[x.name] !== null && r[x.name] !== "";

const job = model("field_service_job", [
  f("name", "text", { required: true }),
  f("customer_id", "ref", { to: "party", required: true }),
  f("scheduled_on", "date"),
  f("site_street", "text"),
  f("site_city", "text"),
  f("price_minor", "money"),
  f("notes", "longtext"),
  f("signature_required", "bool"),
  f("number", "text", { sequence: true }),
  f("health_note", "longtext", { class: "special_category", personal_data: true }),
]);

describe("sections", () => {
  it("groups a long form by kind and name prefix, in a fixed order", () => {
    const r = row({ name: "Fix", customer_id: "p1", site_city: "Hamburg", number: "J-1" });
    const s = sectionsOf(job, job.fields, { creating: false, row: r, filled: filledIn(r) });
    expect(s.map((x) => x.key)).toEqual([
      "main",
      "amounts",
      "pre-^site_",
      "details",
      "notes",
      "sensitive",
      "system",
    ]);
    const by = Object.fromEntries(s.map((x) => [x.key, x]));
    expect(by.main!.fields.map((x) => x.name)).toEqual(["name", "customer_id", "scheduled_on"]);
    expect(by["pre-^site_"]!.label).toBe("Site");
    // Empty optional groups, system values and sensitive data start closed; filled ones open.
    expect(by["pre-^site_"]!.collapsed).toBe(false);
    expect(by.amounts!.collapsed).toBe(true);
    expect(by.system!.collapsed).toBe(true);
    expect(by.sensitive!.collapsed).toBe(true);
    expect(by.sensitive!.personal).toBe(true);
  });

  it("keeps a short form as one group without a heading", () => {
    const m = model("tag", [f("name", "text", { required: true }), f("color", "number")]);
    const s = sectionsOf(m, m.fields, { creating: true, row: null, filled: () => false });
    expect(s).toHaveLength(1);
    expect(s[0]!.label).toBe("");
  });

  it("follows ui.section hints and show_in, the rest falls back", () => {
    const m = model("expense", [f("name", "text"), f("paid_on", "date"), f("note", "longtext")], {
      ui: {
        section: [
          { label: "Claim", fields: ["name"] },
          { label: "Payment", fields: ["paid_on"], show_in: ["paid"] },
        ],
      },
    });
    const draft = sectionsOf(m, m.fields, {
      creating: false,
      row: row({}),
      state: "draft",
      filled: () => false,
    });
    expect(draft.map((x) => x.label)).toEqual(["Claim", "Notes"]);
    const paid = sectionsOf(m, m.fields, {
      creating: false,
      row: row({}),
      state: "paid",
      filled: () => true,
    });
    expect(paid.map((x) => x.label)).toEqual(["Claim", "Payment", "Notes"]);
  });

  it("translates a section the module names, falling back to its English label", async () => {
    provideCatalogue("de", { "section\u0004Review": "Prüfung" });
    await setLanguage("de");
    const m = model("trip", [f("name", "text"), f("note", "text")], {
      ui: {
        section: [
          { label: "Review", fields: ["name"] },
          { label: "Unknown", fields: ["note"] },
        ],
      },
    });
    const s = sectionsOf(m, m.fields, { creating: true, row: null, filled: () => false });
    expect(s.map((x) => x.label)).toEqual(["Prüfung", "Unknown"]);
  });

  it("puts the fields an action asks for under More on a new record", () => {
    const reject: Command = {
      id: "trip.reject",
      description: "",
      needs_approval: false,
      field: "state",
      from: "submitted",
      to: "rejected",
      ui: { needs: ["reject_reason"] },
    };
    const m = model("trip", [...job.fields, f("reject_reason", "longtext")], {
      actions: [reject],
    });
    const s = sectionsOf(m, m.fields, { creating: true, row: null, filled: () => false });
    expect(s.at(-1)!.key).toBe("later");
    expect(s.at(-1)!.fields.map((x) => x.name)).toEqual(["reject_reason"]);
    expect(s.at(-1)!.collapsed).toBe(true);
  });
});

describe("header", () => {
  it("shows computed amounts and totals with a value, at most three", () => {
    const m = model("invoice", [
      f("untaxed_minor", "money", { computed: true }),
      f("tax_minor", "money", { computed: true }),
      f("total_minor", "money", { computed: true }),
      f("unit_price_minor", "money"),
    ]);
    const r = row({ untaxed_minor: 100, tax_minor: 19, total_minor: 119, unit_price_minor: 5 });
    expect(keyFigures(m, m.fields, r).map((x) => x.name)).toEqual([
      "untaxed_minor",
      "tax_minor",
      "total_minor",
    ]);
    expect(keyFigures(m, m.fields, null)).toEqual([]);
  });

  it("leaves a bad end the record is not in out of the steps", () => {
    const state = f("state", "select", { options: ["draft", "posted", "cancelled"] });
    const m = model("move", [state]);
    expect(stepsOf(m, state, "draft")).toEqual(["draft", "posted"]);
    expect(stepsOf(m, state, "cancelled")).toEqual(["draft", "posted", "cancelled"]);
  });
});

describe("locks", () => {
  const state = f("state", "select", { options: ["draft", "posted"] });
  const m = model("move", [state, f("ref", "text")], {
    guard: {
      immutable_when: { field: "state", values: ["posted"] },
      mutable_fields: ["ref"],
      message: "Posted entries cannot be changed. Reverse it to correct it.",
    },
  });

  it("locks a record in a frozen state, with the guard's message and its mutable fields", () => {
    expect(lockOf(m, row({ state: "draft" }))).toBeNull();
    const lock = lockOf(m, row({ state: "posted" }))!;
    expect(lock.message).toContain("Reverse it");
    expect(lock.mutable.has("ref")).toBe(true);
    expect(deleteBlocked(m, row({ state: "posted" }))).toBe(true);
    expect(deleteBlocked(m, row({ state: "draft" }))).toBe(false);
  });

  it("locks every saved record of a guard of mutable fields alone, but those fields", () => {
    const item = model("stock_item", [f("product_id", "text"), f("reorder_level", "number")], {
      guard: { mutable_fields: ["reorder_level"] },
    });
    const lock = lockOf(item, row({ product_id: "p1" }))!;
    expect(lock.mutable.has("reorder_level")).toBe(true);
    expect(lock.mutable.has("product_id")).toBe(false);
    expect(lockOf(item, null)).toBeNull();
  });

  it("locks nothing without a guard", () => {
    expect(lockOf(model("x", [state]), row({ state: "posted" }))).toBeNull();
  });
});

describe("pages and actions", () => {
  it("opens a record with lines or many fields as a page", () => {
    const order = model("sale_order", [f("name", "text")]);
    const line = model("sale_order_line", [f("order_id", "ref", { to: "sale_order" })]);
    const tag = model("tag", [f("name", "text")]);
    const index = indexModels([{ id: "s", name: "S", models: [order, line, tag] }]);
    expect(prefersFullPage(order, index)).toBe(true);
    expect(prefersFullPage(tag, index)).toBe(false);
    expect(prefersFullPage(job, index)).toBe(false);
  });

  it("names an action that reads like a panel button after the model", () => {
    const close: Command = { id: "helpdesk_ticket.close", description: "", needs_approval: false };
    const resolve: Command = {
      id: "helpdesk_ticket.resolve",
      description: "",
      needs_approval: false,
    };
    expect(actionLabelFor(close, "ticket")).toBe("Close ticket");
    expect(actionLabelFor(resolve, "ticket")).toBe("Resolve");
  });

  it("copies what a person writes, without numbers, states and unique values", () => {
    const state = f("state", "select", { options: ["draft", "done"] });
    const m = model("job", [
      f("name", "text", { required: true }),
      f("code", "text", { unique: true }),
      f("number", "text", { sequence: true }),
      state,
      f("note", "longtext"),
    ]);
    const r = row({ name: "Fix", code: "A1", number: "J-1", state: "done", note: "n" });
    expect(copyValues(m, m.fields, r, (x) => x === state)).toEqual({
      name: "Fix (copy)",
      note: "n",
    });
  });

  it("finds the record an action made through a ref it set", () => {
    const m = model("sale_order", [
      f("invoice_id", "ref", { to: "invoice" }),
      f("customer_id", "ref", { to: "party" }),
    ]);
    const before = row({ customer_id: "p1" });
    expect(
      createdRefs(m, before, { invoice_id: "i1", customer_id: "p1" }).map((x) => x.name),
    ).toEqual(["invoice_id"]);
    expect(createdRefs(m, before, null)).toEqual([]);
  });
});

describe("keepShown (#105)", () => {
  it("keeps a section open once drawn open, though its last filled field is emptied", async () => {
    const { keepShown } = await import("./recordLayout.ts");
    const seen = new Set<string>();
    const s = (collapsed: boolean) => [
      { key: "main", label: "", fields: [], collapsed: false, personal: false },
      { key: "details", label: "Details", fields: [], collapsed, personal: false },
      { key: "system", label: "System", fields: [], collapsed: true, personal: false },
    ];
    expect(keepShown(s(false), seen).map((x) => x.collapsed)).toEqual([false, false, true]);
    // Active turned off: the group is empty now, but it stays open.
    expect(keepShown(s(true), seen).map((x) => x.collapsed)).toEqual([false, false, true]);
    // A record never drawn with it open folds it as before.
    expect(keepShown(s(true), new Set()).map((x) => x.collapsed)).toEqual([false, true, true]);
  });
});

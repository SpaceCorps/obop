// lines.ts: the rules of the lines section, with a D95 hint and without one (the server sends
// none today), on the shapes of the built-in modules (sales, accounting, manufacturing, fleet,
// e-commerce).
import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import {
  differenceOf,
  editableField,
  fillsFor,
  fitColumns,
  lineColumns,
  lineDefaults,
  lineFrozen,
  lineOrder,
  linesFrozen,
  shapeOf,
  summedColumns,
} from "./lines.ts";

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
const act = (id: string, field: string, from: string | null, to: string): Command => ({
  id,
  description: "",
  needs_approval: false,
  field,
  from,
  to,
});
const row = (v: Record<string, unknown>): Row => ({
  id: "01a1dc5a-0000-7000-8000-000000000001",
  ...v,
});
const names = (fs: FieldMeta[]) => fs.map((x) => x.name);

const product = model("product", [
  f("name", "text", { required: true }),
  f("sku", "text"),
  f("list_price_minor", "money"),
  f("cost_minor", "money"),
]);
const order = model(
  "sale_order",
  [
    f("number", "text", { sequence: true }),
    f("party_id", "ref", { to: "party" }),
    f("state", "select", { options: ["draft", "confirmed", "cancelled"], governed: true }),
    f("total_minor", "money", { computed: true }),
  ],
  {
    actions: [
      act("sale_order.confirm", "state", "draft", "confirmed"),
      act("sale_order.cancel", "state", null, "cancelled"),
    ],
  },
);
const orderLine = model("sale_order_line", [
  f("order_id", "ref", { to: "sale_order", required: true }),
  f("product_id", "ref", { to: "product" }),
  f("description", "text"),
  f("quantity", "number", { required: true }),
  f("unit_price_minor", "money", { required: true }),
  f("amount_minor", "money", { computed: true }),
]);
const purchaseLine = model("purchase_order_line", [
  f("order_id", "ref", { to: "purchase_order", required: true }),
  f("product_id", "ref", { to: "product", required: true }),
  f("description", "text"),
  f("quantity", "number", { required: true }),
  f("unit_price", "money", { required: true }),
  f("amount", "money", { computed: true }),
]);
const move = model(
  "accounting_move",
  [
    f("journal_id", "ref", { to: "accounting_journal", required: true }),
    f("date", "date", { required: true }),
    f("partner_id", "ref", { to: "party" }),
    f("total_debit", "money", { computed: true }),
    f("total_credit", "money", { computed: true }),
    f("state", "select", {
      options: ["draft", "posted", "cancelled", "reversed"],
      governed: true,
      required: true,
    }),
  ],
  { actions: [act("accounting_move.post", "state", "draft", "posted")] },
);
const moveLine = model("accounting_move_line", [
  f("move_id", "ref", { to: "accounting_move", required: true }),
  f("account_id", "ref", { to: "accounting_account", required: true }),
  f("partner_id", "ref", { to: "party" }),
  f("label", "text"),
  f("debit", "money", { required: true, class: "financial" }),
  f("credit", "money", { required: true, class: "financial" }),
  f("tax_id", "ref", { to: "accounting_tax" }),
  f("due_on", "date"),
  f("reconciled", "bool"),
]);
const service = model(
  "fleet_vehicle_service",
  [
    f("vehicle_id", "ref", { to: "fleet_vehicle", required: true }),
    f("kind", "select", { options: ["maintenance", "repair"], required: true }),
    f("service_on", "date", { required: true }),
    f("cost", "money"),
    f("notes", "longtext"),
    f("state", "select", { options: ["planned", "done", "cancelled"], governed: true }),
  ],
  { actions: [act("fleet_vehicle_service.complete", "state", "planned", "done")] },
);
const image = model("ecommerce_listing_image", [
  f("listing_id", "ref", { to: "ecommerce_listing", required: true }),
  f("image", "image", { required: true }),
  f("alt", "text"),
  f("sequence", "number"),
]);
const listing = model("ecommerce_listing", [f("title", "text", { required: true })]);

describe("shapeOf", () => {
  it("draws plain lines as a table, a child with a workflow as a list, images as a grid", () => {
    expect(
      shapeOf(order, { model: orderLine.name, via: "order_id", shape: "lines" }, orderLine),
    ).toBe("lines");
    expect(
      shapeOf(order, { model: service.name, via: "vehicle_id", shape: "lines" }, service),
    ).toBe("list");
    expect(shapeOf(listing, { model: image.name, via: "listing_id", shape: "images" }, image)).toBe(
      "images",
    );
    expect(
      shapeOf(listing, { model: "x_translation", via: "listing_id", shape: "translations" }, image),
    ).toBe("translations");
  });

  it("takes the shape the server declares", () => {
    const declared = { ...order, children: [{ model: service.name, via: "vehicle_id" }] };
    expect(
      shapeOf(declared, { model: service.name, via: "vehicle_id", shape: "lines" }, service),
    ).toBe("lines");
  });
});

describe("lineColumns", () => {
  it("orders refs, text, quantities, prices and the computed amount last, without the parent", () => {
    expect(names(lineColumns(orderLine, "order_id"))).toEqual([
      "product_id",
      "description",
      "quantity",
      "unit_price_minor",
      "amount_minor",
    ]);
    expect(names(lineColumns(moveLine, "move_id"))).toEqual([
      "account_id",
      "partner_id",
      "tax_id",
      "label",
      "debit",
      "credit",
      "due_on",
      "reconciled",
    ]);
  });

  it("leaves images, long text and positions to the sheet", () => {
    expect(names(lineColumns(image, "listing_id"))).toEqual(["alt"]);
    expect(names(lineColumns(service, "vehicle_id"))).toEqual([
      "cost",
      "service_on",
      "kind",
      "state",
    ]);
  });

  it("follows ui.list when the child declares it", () => {
    const hinted = { ...orderLine, ui: { list: ["description", "quantity", "order_id"] } };
    expect(names(lineColumns(hinted, "order_id"))).toEqual(["description", "quantity"]);
  });
});

describe("fitColumns", () => {
  const cols = lineColumns(moveLine, "move_id");
  it("keeps every column when there is room", () => {
    const fit = fitColumns(cols, 1400);
    expect(fit.fits).toBe(true);
    expect(fit.hidden).toEqual([]);
  });

  it("drops switches, dates and optional refs first, and keeps the required columns", () => {
    const fit = fitColumns(cols, 420);
    expect(fit.fits).toBe(true);
    expect(names(fit.shown)).toEqual(["account_id", "debit", "credit"]);
    expect(names(fit.hidden)).toEqual(["partner_id", "tax_id", "label", "due_on", "reconciled"]);
  });

  it("says when even the required columns do not fit (the section draws cards)", () => {
    expect(fitColumns(lineColumns(orderLine, "order_id"), 330).fits).toBe(false);
  });
});

describe("editableField", () => {
  it("refuses the parent ref, computed, numbered, read-only and governed fields", () => {
    const by = (m: ModelMeta, n: string) => m.fields.find((x) => x.name === n)!;
    expect(editableField(orderLine, by(orderLine, "order_id"), "order_id")).toBe(false);
    expect(editableField(orderLine, by(orderLine, "amount_minor"), "order_id")).toBe(false);
    expect(editableField(orderLine, by(orderLine, "quantity"), "order_id")).toBe(true);
    expect(editableField(service, by(service, "state"), "vehicle_id")).toBe(false);
    const ro = { ...orderLine, fields: [f("quantity", "number", { readonly: true })] };
    expect(editableField(ro, ro.fields[0]!, "order_id")).toBe(false);
  });
});

describe("lineDefaults", () => {
  it("sets the parent, a quantity of 1 and the parent's partner", () => {
    const d = lineDefaults({
      child: orderLine,
      via: "order_id",
      parent: order,
      parentRow: row({ party_id: "P" }),
      parentId: "O",
      siblings: [],
    });
    expect(d).toEqual({ order_id: "O", quantity: 1 });
    const m = lineDefaults({
      child: moveLine,
      via: "move_id",
      parent: move,
      parentRow: row({ partner_id: "P" }),
      parentId: "M",
      siblings: [],
    });
    expect(m).toMatchObject({ move_id: "M", partner_id: "P", debit: 0, credit: 0 });
  });

  it("proposes the amount that balances a journal entry", () => {
    const base = { child: moveLine, via: "move_id", parent: move, parentRow: null, parentId: "M" };
    expect(lineDefaults({ ...base, siblings: [{ debit: 10000, credit: 0 }] })).toMatchObject({
      debit: 0,
      credit: 10000,
    });
    expect(
      lineDefaults({
        ...base,
        siblings: [
          { debit: 0, credit: 2500 },
          { debit: 1000, credit: 0 },
        ],
      }),
    ).toMatchObject({ debit: 1500, credit: 0 });
  });

  it("starts a required choice at its first option", () => {
    const d = lineDefaults({
      child: service,
      via: "vehicle_id",
      parent: order,
      parentRow: null,
      parentId: "V",
      siblings: [],
    });
    expect(d.kind).toBe("maintenance");
    expect("state" in d).toBe(false);
  });
});

describe("fillsFor", () => {
  const ref = orderLine.fields[1]!;
  const picked = row({ name: "Drill", list_price_minor: 24900, cost_minor: 15000 });
  it("fills the description and the selling price from a picked product", () => {
    expect(fillsFor({ child: orderLine, ref, target: product, picked, label: "Drill" })).toEqual({
      description: "Drill",
      unit_price_minor: 24900,
    });
  });

  it("takes the cost on a purchase line", () => {
    const pref = purchaseLine.fields[1]!;
    expect(
      fillsFor({ child: purchaseLine, ref: pref, target: product, picked, label: "Drill" }),
    ).toEqual({ description: "Drill", unit_price: 15000 });
  });

  it("fills nothing from a record without a price (an account)", () => {
    const account = model("accounting_account", [f("code", "text"), f("name", "text")]);
    expect(
      fillsFor({
        child: moveLine,
        ref: moveLine.fields[1]!,
        target: account,
        picked: row({ code: "8400", name: "Revenue" }),
        label: "8400",
      }),
    ).toEqual({});
  });

  it("follows fill_from when the child declares any", () => {
    const hinted = {
      ...orderLine,
      fields: orderLine.fields.map((x) =>
        x.name === "unit_price_minor" ? { ...x, fill_from: "product_id.cost_minor" } : x,
      ),
    };
    expect(fillsFor({ child: hinted, ref, target: product, picked, label: "Drill" })).toEqual({
      unit_price_minor: 15000,
    });
  });
});

describe("linesFrozen", () => {
  it("freezes the lines of a posted entry, not of a draft or a confirmed order (no guard sent)", () => {
    expect(linesFrozen(move, row({ state: "posted" }), moveLine, "move_id").frozen).toBe(true);
    expect(linesFrozen(move, row({ state: "draft" }), moveLine, "move_id").frozen).toBe(false);
    expect(linesFrozen(order, row({ state: "confirmed" }), orderLine, "order_id").frozen).toBe(
      false,
    );
    expect(linesFrozen(order, row({ state: "cancelled" }), orderLine, "order_id").frozen).toBe(
      true,
    );
    expect(linesFrozen(move, null, moveLine, "move_id").frozen).toBe(false);
  });

  it("follows the D77 guard when meta sends one", () => {
    const guarded = {
      ...move,
      guard: {
        immutable_when: { field: "state", values: ["posted"] },
        message: "Posted entries cannot be changed. Reverse it to correct it.",
      },
    };
    const child = { ...moveLine, guard: { children_of: "move_id" } };
    expect(linesFrozen(guarded, row({ state: "posted" }), child, "move_id")).toEqual({
      frozen: true,
      message: "Posted entries cannot be changed. Reverse it to correct it.",
    });
    // A guard that does not cover the children leaves them open, also when cancelled.
    expect(linesFrozen(guarded, row({ state: "posted" }), moveLine, "move_id").frozen).toBe(false);
    expect(linesFrozen(guarded, row({ state: "cancelled" }), child, "move_id").frozen).toBe(false);
  });

  it("freezes one line by its own guard", () => {
    const own = {
      ...moveLine,
      guard: { immutable_when: { field: "reconciled", values: ["true"] } },
    };
    expect(lineFrozen(own, row({ reconciled: true }))).toBe(true);
    expect(lineFrozen(own, row({ reconciled: false }))).toBe(false);
    expect(lineFrozen({ ...moveLine, guard: { append_only: true } }, row({}))).toBe(true);
  });
});

describe("sums and order", () => {
  it("adds up amounts, not unit prices or quantities", () => {
    expect(names(summedColumns(orderLine, lineColumns(orderLine, "order_id")))).toEqual([
      "amount_minor",
    ]);
    expect(names(summedColumns(moveLine, lineColumns(moveLine, "move_id")))).toEqual([
      "debit",
      "credit",
    ]);
  });

  it("tells the difference between debit and credit", () => {
    expect(
      differenceOf(moveLine, [
        { debit: 100, credit: 0 },
        { debit: 0, credit: 40 },
      ]),
    ).toBe(60);
    expect(
      differenceOf(moveLine, [
        { debit: "100", credit: 0 },
        { debit: 0, credit: 100 },
      ]),
    ).toBe(0);
    expect(differenceOf(orderLine, [])).toBeNull();
  });

  it("lists by position, else in the order the lines were made", () => {
    expect(lineOrder(image)).toEqual([
      { field: "sequence", dir: "asc" },
      { field: "id", dir: "asc" },
    ]);
    expect(lineOrder(orderLine)).toEqual([{ field: "id", dir: "asc" }]);
  });
});

// Q1: the "On hand" smart button of a product leaves out stock in scrap locations.
import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, ModelMeta } from "./api.ts";
import { notIn, scrapRef } from "./RelatedChips.tsx";

const f = (name: string, kind: FieldMeta["kind"], more: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...more,
});
const m = (name: string, fields: FieldMeta[]): ModelMeta => ({
  name,
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields,
  actions: [],
});
const location = m("inventory_location", [
  f("name", "text"),
  f("kind", "select", { options: ["internal", "receiving", "scrap"] }),
]);
const item = m("inventory_item", [
  f("product_id", "ref", { to: "product" }),
  f("location_id", "ref", { to: "inventory_location" }),
  f("quantity", "number"),
]);
const models = new Map([location, item, m("product", [f("name", "text")])].map((x) => [x.name, x]));

describe("the stock a smart button sums (Q1)", () => {
  it("finds the place ref whose kind has scrap, and leaves those places out", () => {
    expect(scrapRef(item, (n) => models.get(n))?.name).toBe("location_id");
    expect(scrapRef(models.get("product"), (n) => models.get(n))).toBeUndefined();
    expect(notIn("location_id", ["s1", "s2"])).toEqual([
      { field: "location_id", op: "ne", value: "s1" },
      { field: "location_id", op: "ne", value: "s2" },
    ]);
  });
});

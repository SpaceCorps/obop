// A dashboard tile split by a date counts per the period it names, month when it names none (K1).
import { describe, expect, it } from "vite-plus/test";
import type { ModelMeta } from "../api.ts";
import { resolve, widgetDim, type Widget } from "./widget.ts";

const orders: ModelMeta = {
  name: "sale_order",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    {
      name: "ordered_at",
      kind: "datetime",
      description: "",
      class: "none",
      personal_data: false,
      required: true,
    },
  ],
  actions: [],
};
const models = new Map([["sale_order", { model: orders }]]);
const tile = (period?: Widget["period"]): Widget => ({
  id: "w1",
  dashboard_id: "d1",
  title: "Orders",
  kind: "line",
  model_name: "sale_order",
  aggregate: "count",
  group_by: "ordered_at",
  period,
});
const bucketOf = (w: Widget) => {
  const r = resolve(w, models);
  if (!r.ok) throw new Error(r.detail);
  return widgetDim(r)?.bucket;
};

describe("the period of a dated split", () => {
  it("is month when the tile names none", () => {
    expect(bucketOf(tile())).toBe("month");
    expect(bucketOf(tile(null))).toBe("month");
  });
  it("is the one the tile names", () => {
    expect(bucketOf(tile("iso_week"))).toBe("iso_week");
    expect(bucketOf(tile("year"))).toBe("year");
    expect(bucketOf(tile("day"))).toBe("day");
  });
});

// The address seams of route.ts: the app overview page and the full-page record flag.
import { describe, expect, it } from "vite-plus/test";
import { OVERVIEW, href, isFull, isOverview, overviewHref, parse, withFull } from "./route.ts";

describe("route seams", () => {
  it("an app's overview is #/<app>/overview", () => {
    expect(overviewHref("sales")).toBe("#/sales/overview");
    expect(isOverview(parse("#/sales/overview"))).toBe(true);
    expect(isOverview(parse("#/sales/sale_order"))).toBe(false);
    expect(isOverview({ app: "customize", model: OVERVIEW })).toBe(false);
  });
  it("&full=1 opens the record as a page, and survives a round trip", () => {
    const r = parse("#/sales/sale_order?id=abc&full=1");
    expect(r.full).toBe("1");
    expect(isFull(r)).toBe(true);
    expect(href(r)).toBe("#/sales/sale_order?id=abc&full=1");
    expect(href(withFull(r, false))).toBe("#/sales/sale_order?id=abc");
    expect(href(withFull({ app: "crm", model: "lead", id: "x" }, true))).toBe(
      "#/crm/lead?id=x&full=1",
    );
  });
  it("any other value of full is the panel", () => {
    expect(parse("#/crm/lead?full=yes").full).toBe("");
    expect(parse("#/crm/lead").full).toBe("");
  });
});

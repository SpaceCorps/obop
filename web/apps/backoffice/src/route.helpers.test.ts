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

describe("a field named in the address (Q19)", () => {
  it("is read and written; a name that is not a field name is dropped", async () => {
    const { href, parse } = await import("./route.ts");
    expect(parse("#/accounting_se/vat_code?id=c1&field=tax_id").field).toBe("tax_id");
    expect(href({ app: "a", model: "m", id: "c1", field: "tax_id" })).toBe(
      "#/a/m?id=c1&field=tax_id",
    );
    expect(parse("#/a/m?id=c1&field=%3Cx%3E").field).toBe("");
  });
});

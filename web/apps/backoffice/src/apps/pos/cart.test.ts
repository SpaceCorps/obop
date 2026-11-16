import { describe, expect, it } from "vite-plus/test";
import {
  addProduct,
  byCode,
  cartTotal,
  cashTender,
  divRound,
  lineTotal,
  matches,
  percentToBp,
  quickCash,
  remaining,
  setQty,
} from "./cart.ts";

describe("POS cart", () => {
  it("computes a line the way the server does", () => {
    // Seeded line: 10 boxes at 14.90 with 5 % off is 141.55 on the server.
    expect(lineTotal({ qty: 10, price: 1490, discountBp: 500 })).toBe(14155);
    expect(lineTotal({ qty: 1, price: 22900, discountBp: 500 })).toBe(21755);
    // Half away from zero: 1 x 1.01 with 50 % off is 0.505, which is 0.51.
    expect(lineTotal({ qty: 1, price: 101, discountBp: 5000 })).toBe(51);
    expect(divRound(-5, 10)).toBe(-1);
  });

  it("adds a second tap to the same line and removes a line at zero", () => {
    let c = addProduct([], { id: "a", name: "Drill", price: 24900 });
    c = addProduct(c, { id: "a", name: "Drill", price: 24900 });
    c = addProduct(c, { id: "b", name: "Bits", price: 990 });
    expect(c.map((l) => l.qty)).toEqual([2, 1]);
    expect(cartTotal(c)).toBe(50790);
    expect(setQty(c, 0, 0).map((l) => l.product)).toEqual(["b"]);
  });

  it("reads a discount as a percentage", () => {
    expect(percentToBp("10")).toBe(1000);
    expect(percentToBp("12,5")).toBe(1250);
    expect(percentToBp("")).toBe(0);
    expect(percentToBp("101")).toBeNull();
    expect(percentToBp("abc")).toBeNull();
  });

  it("gives change for cash and offers round amounts", () => {
    expect(cashTender(5000, 3590)).toEqual({ amount: 3590, change: 1410 });
    expect(cashTender(2000, 3590)).toEqual({ amount: 2000, change: 0 });
    expect(quickCash(3590)).toEqual([3590, 4000, 5000, 10000]);
    expect(quickCash(0)).toEqual([]);
    expect(remaining(3590, [{ method: "card", amount: 4000 }])).toBe(0);
  });

  it("finds products by name words and by code", () => {
    const p = { name: "Cordless drill driver 18 V", sku: "PWR-1801" };
    expect(matches(p, "drill 18")).toBe(true);
    expect(matches(p, "pwr-1801")).toBe(true);
    expect(matches(p, "hammer")).toBe(false);
    expect(byCode([p], " PWR-1801 ")).toBe(p);
    expect(byCode([p], "PWR")).toBeUndefined();
  });
});

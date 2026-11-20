import { describe, expect, it } from "vite-plus/test";
import { heldTo, outsideFirst, type StoredLimit } from "./StoredLimits.tsx";

const row = (over: Partial<StoredLimit>): StoredLimit => ({
  module: "purchase",
  model: "purchase_vendor",
  field: "name",
  records: 6,
  outside: 0,
  longest: null,
  max_length: null,
  min_length: null,
  min: null,
  max: null,
  pattern: null,
  ...over,
});

describe("stored limits (K46)", () => {
  it("says what a field is held to", () => {
    expect(heldTo(row({ pattern: "iban", max_length: 2048 }))).toBe(
      "IBAN · at most 2,048 characters",
    );
    expect(heldTo(row({ min_length: 1, max_length: 10 }))).toBe("1 to 10 characters");
    expect(heldTo(row({ min: "0", max: "9" }))).toBe("from 0 to 9");
    expect(heldTo(row({ max: "9.99" }))).toBe("at most 9.99");
  });

  it("lists only the fields with values outside, the worst first", () => {
    const rows = [
      row({ field: "a", outside: 0 }),
      row({ field: "iban", outside: 2 }),
      row({ field: "code", outside: 5 }),
    ];
    expect(outsideFirst(rows).map((r) => r.field)).toEqual(["code", "iban"]);
  });
});

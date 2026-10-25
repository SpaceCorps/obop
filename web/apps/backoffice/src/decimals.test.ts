import { describe, expect, it } from "vite-plus/test";
import {
  canonicalDecimal,
  compareDecimals,
  decimalInput,
  formatDecimal,
  isDecimalText,
  sumDecimals,
} from "./decimals.ts";

describe("exact decimals", () => {
  it("recognises the server's text form and nothing looser", () => {
    for (const ok of ["0", "12", "-12.50", "0.05", "007.1"])
      expect(isDecimalText(ok), ok).toBe(true);
    for (const bad of [
      "",
      " 1",
      "1 ",
      "1,5",
      "1e3",
      "+1",
      ".5",
      "5.",
      "NaN",
      "0x10",
      "--1",
      null,
      5,
    ])
      expect(isDecimalText(bad), String(bad)).toBe(false);
  });

  it("writes typed parts canonically", () => {
    expect(canonicalDecimal(false, "12", "5", 2)).toBe("12.50");
    expect(canonicalDecimal(true, "007", "", 2)).toBe("-7.00");
    expect(canonicalDecimal(true, "0", "00", 2)).toBe("0.00");
    expect(canonicalDecimal(true, "12", "", 0)).toBe("-12");
    expect(canonicalDecimal(false, "", "", 3)).toBe("0.000");
  });

  it("orders by value, not by text", () => {
    expect(compareDecimals("9.5", "10.25")).toBe(-1);
    expect(compareDecimals("10.25", "9.5")).toBe(1);
    expect(compareDecimals("1.0", "1.00")).toBe(0);
    expect(compareDecimals("-10", "-9.99")).toBe(-1);
    expect(compareDecimals("0.000000000001", "0")).toBe(1);
    const sorted = ["10.25", "9.5", "-3", "0"].sort(compareDecimals);
    expect(sorted).toEqual(["-3", "0", "9.5", "10.25"]);
  });

  it("adds exactly", () => {
    expect(sumDecimals(["0.1", "0.2"])).toBe("0.3");
    expect(sumDecimals(["0.10", "0.2"])).toBe("0.30");
    expect(sumDecimals(["-5.05", "5"])).toBe("-0.05");
    expect(sumDecimals([])).toBe("0");
    // 1,000,000 values of scale 2: the sum is exact where a float sum drifts.
    const many = Array.from(
      { length: 1_000_000 },
      (_, i) => `${i % 1000}.${String(i % 100).padStart(2, "0")}`,
    );
    let oracle = 0n;
    for (const v of many) oracle += BigInt(v.replace(".", ""));
    const s = oracle.toString().padStart(3, "0");
    expect(sumDecimals(many)).toBe(`${s.slice(0, -2)}.${s.slice(-2)}`);
  });

  it("formats in the locale from the digits", () => {
    expect(formatDecimal("1234567.5", "en", 2)).toBe("1,234,567.50");
    expect(formatDecimal("1234567.5", "de", 2)).toBe("1.234.567,50");
    expect(formatDecimal("-0.05", "en", 2)).toBe("-0.05");
    // More digits than a double holds survive.
    expect(formatDecimal("12345678901234567.89", "en", 2)).toBe("12,345,678,901,234,567.89");
    expect(formatDecimal(null, "en")).toBe("");
    expect(formatDecimal("oops", "en")).toBe("oops");
  });

  it("puts the locale's mark in an input box", () => {
    expect(decimalInput("12.50", ",")).toBe("12,50");
    expect(decimalInput("12.50", ".")).toBe("12.50");
    expect(decimalInput(undefined, ",")).toBe("");
  });
});

import { afterEach, describe, expect, it } from "vite-plus/test";
import { setServerCurrencies } from "./api.ts";
import { matchesPattern } from "./patterns.ts";
import fixtures from "./patterns.fixtures.json";

describe("named formats as the server checks them (U18)", () => {
  afterEach(() => setServerCurrencies(undefined));

  it("answers every shared case as the server does", () => {
    setServerCurrencies(["CHF", "EUR", "SEK"]);
    for (const c of fixtures.cases)
      expect([c.pattern, c.value, matchesPattern(c.pattern, c.value)]).toEqual([
        c.pattern,
        c.value,
        c.ok,
      ]);
  });

  it("leaves a currency code to the server when it sent no list", () => {
    expect(matchesPattern("currency_code", "ZZZ")).toBe(true);
  });
});

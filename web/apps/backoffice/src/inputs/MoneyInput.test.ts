import { afterEach, describe, expect, it } from "vite-plus/test";
import { setServerCurrencies } from "../api.ts";
import { currencyCodes } from "./MoneyInput.tsx";

describe("currencyCodes (U15)", () => {
  afterEach(() => setServerCurrencies(undefined));

  it("offers the codes the server accepts, not the ones the browser knows", () => {
    setServerCurrencies(["CHF", "EUR", "SEK", "XAU"]);
    expect(currencyCodes()).toEqual(["CHF", "EUR", "SEK", "XAU"]);
  });

  it("drops what is not a code and falls back to the browser for an older server", () => {
    setServerCurrencies(["EUR", 7, null]);
    expect(currencyCodes()).toEqual(["EUR"]);
    setServerCurrencies(undefined);
    const fallback = currencyCodes();
    expect(fallback).toContain("EUR");
    expect(fallback.length).toBeGreaterThan(1);
  });
});

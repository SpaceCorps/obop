import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { lastMonth, mayBillTime, rateHint } from "./BillTime.tsx";

describe("Bill time", () => {
  afterEach(async () => {
    await setLanguage("en", false);
  });

  it("gives the example rate with the decimal mark of the language", async () => {
    await setLanguage("en", false);
    expect(rateHint("EUR")).toContain("95.00");
    for (const lang of ["de", "fr", "pl", "fi", "sv", "es"] as const) {
      const url = new URL(`../../../locales/${lang}/messages.json`, import.meta.url);
      provideCatalogue(lang, JSON.parse(readFileSync(url, "utf8")));
      await setLanguage(lang, false);
      const hint = rateHint("EUR");
      expect(hint, lang).toContain("95,00");
      expect(hint, lang).not.toContain("95.00");
    }
  });

  const invoice = { model: { can_create: true, actions: [] } };
  const entry = (ids: string[]) => ({
    model: {
      can_create: true,
      actions: ids.map((id) => ({ id, description: "", needs_approval: false })),
    },
  });

  it("is offered only to a person who may make invoices and approve time (the server's groups)", () => {
    // invoicing.user and timesheets.user without timesheets.manager: /api/meta has no approve.
    const submitOnly = new Map([
      ["invoice", invoice],
      ["timesheets_entry", entry(["timesheets_entry.submit"])],
    ]);
    expect(mayBillTime(submitOnly)).toBe(false);
    const manager = new Map([
      ["invoice", invoice],
      ["timesheets_entry", entry(["timesheets_entry.submit", "timesheets_entry.approve"])],
    ]);
    expect(mayBillTime(manager)).toBe(true);
    const noInvoices = new Map([
      ["invoice", { model: { can_create: false, actions: [] } }],
      ["timesheets_entry", entry(["timesheets_entry.approve"])],
    ]);
    expect(mayBillTime(noInvoices)).toBe(false);
    expect(mayBillTime(new Map([["invoice", invoice]]))).toBe(false);
  });

  it("offers the month before as the period", () => {
    expect(lastMonth(new Date(2026, 10, 17))).toEqual({ from: "2026-10-01", to: "2026-10-31" });
    // January looks back into December of the year before.
    expect(lastMonth(new Date(2026, 0, 5))).toEqual({ from: "2025-12-01", to: "2025-12-31" });
    expect(lastMonth(new Date(2028, 2, 1))).toEqual({ from: "2028-02-01", to: "2028-02-29" });
  });
});

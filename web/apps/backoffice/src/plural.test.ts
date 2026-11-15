// plural.ts: the plural of a model's name for headings and empty states.
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { ModelMeta } from "./api.ts";
import { englishPlural, pluralTitle } from "./plural.ts";

afterEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en");
});

const model = (more: Partial<ModelMeta> = {}): ModelMeta => ({
  name: "sale_order",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [],
  actions: [],
  ...more,
});

describe("englishPlural", () => {
  it("follows the common English rules", () => {
    expect(englishPlural("Sale order")).toBe("Sale orders");
    expect(englishPlural("Party")).toBe("Parties");
    expect(englishPlural("Journey")).toBe("Journeys");
    expect(englishPlural("Tax")).toBe("Taxes");
    expect(englishPlural("Batch")).toBe("Batches");
    expect(englishPlural("Address")).toBe("Addresses");
    expect(englishPlural("Status")).toBe("Statuses");
    expect(englishPlural("Analysis")).toBe("Analyses");
    expect(englishPlural("BOM")).toBe("BOMs");
  });
  it("leaves plurals and uncountable words alone", () => {
    expect(englishPlural("Settings")).toBe("Settings");
    expect(englishPlural("Equipment")).toBe("Equipment");
    expect(englishPlural("")).toBe("");
  });
});

describe("pluralTitle", () => {
  it("the package's plural wins, then the English rules", () => {
    expect(pluralTitle(model({ plural: "Quotations" }), "Sale order")).toBe("Quotations");
    expect(pluralTitle(model(), "Sale order")).toBe("Sale orders");
  });
  it("another language keeps the name unless the package gives its plural", async () => {
    provideCatalogue("de", {});
    await setLanguage("de");
    expect(pluralTitle(model(), "Verkaufsauftrag")).toBe("Verkaufsauftrag");
    expect(
      pluralTitle(model({ plural_labels: { de: "Verkaufsaufträge" } }), "Verkaufsauftrag"),
    ).toBe("Verkaufsaufträge");
  });
});

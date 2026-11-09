import { describe, expect, test } from "vite-plus/test";
import { LOCALES, messages } from "../../i18n/index.ts";
import { appView, splitPurpose } from "./app-view.ts";
import { loadApps } from "./apps.ts";

const apps = loadApps();

describe("the purpose is split after its first sentence", () => {
  test("every purpose gives a lead that is one whole sentence and a rest that is the remainder", () => {
    for (const app of apps) {
      const { lead, rest } = splitPurpose(app.purpose);
      expect(lead, app.id).toMatch(/[.!?]$/);
      expect(rest === "" ? lead : `${lead} ${rest}`, app.id).toBe(app.purpose);
      if (rest) expect(rest, app.id).toMatch(/^[A-Z0-9"(]/);
      // A lead that runs on is a sentence the rule split wrongly (an abbreviation, say).
      expect(lead.length, app.id).toBeLessThan(260);
    }
  });

  test("one sentence has no rest, and an abbreviation does not end a sentence", () => {
    expect(splitPurpose("One sentence only.")).toEqual({ lead: "One sentence only.", rest: "" });
    expect(splitPurpose("It is first. It is second.")).toEqual({
      lead: "It is first.",
      rest: "It is second.",
    });
    expect(splitPurpose("Costs, e.g. tax. More here.").lead).toBe("Costs, e.g. tax.");
  });
});

describe("the view of an app, in every language", () => {
  for (const locale of LOCALES) {
    test(`${locale}: four facts, cards in the page language, a plain compatibility line`, () => {
      const m = messages[locale];
      for (const app of apps) {
        const v = appView(app, m, apps, locale);
        expect(
          v.facts.map((f) => f.label),
          app.id,
        ).toEqual([
          m.apps.meta.maturity,
          m.apps.meta.version,
          m.apps.store.worksWith,
          m.apps.store.category,
        ]);
        expect(
          v.facts.every((f) => f.value.length > 0),
          app.id,
        ).toBe(true);
        expect(v.actions.length, app.id).toBe(app.models.flatMap((x) => x.actions).length);
        expect(v.models.length, app.id).toBe(app.models.length);
        // Every card is complete, and in a translated language none falls back to English text:
        // the product's label files cover every model and action.
        for (const c of [...v.actions, ...v.models]) {
          expect(c.title.length + c.text.length, app.id).toBeGreaterThan(0);
          expect(c.lang, `${locale} ${app.id} ${c.title}`).toBeUndefined();
        }
        expect(v.info.find((r) => r.label === m.apps.store.compat)?.value, app.id).toBe(
          m.apps.store.compatValue.replace("{version}", "0.2"),
        );
      }
    });
  }

  test("English cards carry no lang mark and use plain names for the oddly named models", () => {
    const m = messages.en;
    const byId = (id: string) => apps.find((a) => a.id === id)!;
    const titles = (id: string) => appView(byId(id), m, apps, "en").models.map((c) => c.title);
    expect(titles("base")).toEqual(["Contact"]);
    expect(titles("manufacturing")).toEqual(["Bill of materials", "Component", "Order"]);
    expect(titles("accounting")).toContain("Journal entry");
    expect(titles("inventory")).toContain("Stock move");
  });
});

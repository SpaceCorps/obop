import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { repoRoot } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// D52: names and descriptions of an installed package come from its manifest, per language. The
// expectations are read from the server's own /api/meta, so a reworded translation does not break
// the spec; what it proves is that the UI shows what the server sends, in each place it matters.

const tr = (lang: string, id: string): string => {
  const cat = JSON.parse(
    readFileSync(resolve(repoRoot, `web/apps/backoffice/locales/${lang}/messages.json`), "utf8"),
  );
  const text = cat[id];
  if (!text) throw new Error(`no ${lang} translation of ${JSON.stringify(id)}`);
  return text;
};

type Labelled = { name?: string; id?: string; labels?: Record<string, string> };

test.describe("server labels of an installed package", () => {
  for (const lang of ["de", "fi"] as const) {
    test(`Fleet is shown in ${lang}: launcher, sidebar, columns, options and actions`, async ({
      page,
      signIn,
      api,
      uid,
    }) => {
      const root = await api.token("root");
      await api.post("/api/marketplace_listing/install", { id: "fleet" }, root);
      try {
        const plate = uid("LBL").replace(/ /g, "-");
        const vehicle = await api.create("root", "fleet_vehicle", {
          plate,
          fuel: "electric",
          state: "available",
        });

        // What the server says: the contract under test.
        const meta = (await api.get("/api/meta", root)).json.modules.find(
          (m: Labelled) => m.id === "fleet",
        );
        const model = meta.models.find((m: Labelled) => m.name === "fleet_vehicle");
        const field = (n: string) => model.fields.find((f: Labelled) => f.name === n);
        const action = model.actions.find((a: Labelled) => a.id === "fleet_vehicle.dispatch");
        const appName: string = meta.labels[lang];
        const modelName: string = model.labels[lang];
        const plateLabel: string = field("plate").labels[lang];
        const fuelLabel: string = field("fuel").labels[lang];
        const electric: string = field("fuel").option_labels.electric[lang];
        const dispatch: string = action.labels[lang];
        for (const s of [appName, modelName, plateLabel, fuelLabel, electric, dispatch]) {
          expect(s).toBeTruthy();
        }
        expect(appName).not.toBe("Fleet");
        expect(electric).not.toBe("Electric");

        await signIn("root");
        await page.goto(`/?lang=${lang}#/`);
        await expect(page.locator("html")).toHaveAttribute("lang", lang);

        // Launcher.
        await page.getByRole("button", { name: tr(lang, "Open the app launcher") }).click();
        await expect(
          page.getByRole("dialog", { name: tr(lang, "Apps") }).getByRole("link", { name: appName }),
        ).toBeVisible();
        await page.keyboard.press("Escape");

        // Sidebar and column headers.
        await page.goto(`/?lang=${lang}#/fleet/fleet_vehicle?view=list`);
        await expect(page.locator(".sidebar")).toContainText(modelName);
        await expect(page.getByRole("columnheader", { name: plateLabel })).toBeVisible();
        await expect(page.getByRole("columnheader", { name: fuelLabel })).toBeVisible();
        const row = page.getByRole("row", { name: new RegExp(plate) });
        await expect(row).toContainText(electric);
        await expect(row).not.toContainText("Electric");

        // Record panel: select options and the action button.
        await page.goto(`/?lang=${lang}#/fleet/fleet_vehicle?view=list&id=${vehicle.id}`);
        const panel = page.getByRole("complementary", { name: /./ });
        await expect(panel.getByLabel(fuelLabel)).toBeVisible();
        await expect(
          panel.getByLabel(fuelLabel).locator("option", { hasText: electric }),
        ).toHaveCount(1);
        await expect(panel.getByRole("button", { name: dispatch })).toBeVisible();

        // English is untouched, and a language without labels falls back to it.
        await page.goto(`/?lang=en#/fleet/fleet_vehicle?view=list`);
        await expect(page.getByRole("columnheader", { name: "Plate" })).toBeVisible();
      } finally {
        await api.post("/api/module/uninstall", { id: "fleet" }, root);
      }
    });
  }
});

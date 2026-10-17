import { mkdirSync, writeFileSync } from "node:fs";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

// A manual-style QA sweep (not part of the suite: it only runs with OBOP_QA=1). It visits every
// screen at four widths, in light and dark, in English and German, and reports layout problems; the
// console guard of the fixtures fails on any console or page error.
const OUT = process.env.OBOP_QA_OUT ?? "/tmp/obop-qa";
const widths = [
  [1440, 900],
  [1024, 768],
  [768, 1024],
  [390, 844],
] as const;

test.skip(!process.env.OBOP_QA, "set OBOP_QA=1 to run the sweep");

for (const lang of ["en", "de"]) {
  for (const scheme of ["light", "dark"] as const) {
    for (const [w, h] of widths) {
      test(`sweep ${lang} ${scheme} ${w}`, async ({ page, signIn, api, scratchModule }) => {
        test.setTimeout(240_000);
        mkdirSync(OUT, { recursive: true });
        const model = await scratchModule({
          title: "QA deals",
          fields: [
            { name: "title", required: true },
            { name: "state", kind: "select", options: ["new", "qualified", "won", "lost"] },
            { name: "amount_minor", kind: "money" },
            { name: "due", kind: "date" },
          ],
        });
        for (let i = 0; i < 45; i++)
          await api.create("root", model, {
            title: `Deal ${i}`,
            state: ["new", "qualified", "won", "lost"][i % 4],
            amount_minor: 1000 * (i + 1),
            due: `2026-10-${String(1 + (i % 28)).padStart(2, "0")}`,
          });
        await page.emulateMedia({ colorScheme: scheme });
        await page.setViewportSize({ width: w, height: h });
        await page.addInitScript((l) => localStorage.setItem("obop-lang", l), lang);
        await signIn("root", "/");
        const routes: [string, string][] = [
          ["home", "/"],
          ["list", `/${model}/${model}?view=list`],
          ["kanban", `/${model}/${model}?view=kanban`],
          ["pivot", `/${model}/${model}?view=pivot`],
          ["graph", `/${model}/${model}?view=graph`],
          ["calendar", `/${model}/${model}?view=calendar`],
          ["crm", "/crm/lead"],
          ["party", "/base/party"],
          ["theme", "/customize/theme?tab=theme"],
          ["apps", "/customize/apps?tab=apps"],
          ["modules", "/customize/modules?tab=modules"],
          ["marketplace", "/customize/modules?tab=modules&view=marketplace"],
          ["build", "/customize/build?tab=build"],
          ["security", "/customize/security?tab=security"],
          ["brand", "/customize/brand?tab=brand"],
          ["guide", "/customize/guide?tab=guide"],
        ];
        const report: string[] = [];
        for (const [name, hash] of routes) {
          await page.goto(`/#${hash}`);
          await page.reload();
          await expect(page.locator(".topbar")).toBeVisible();
          await page.waitForTimeout(700);
          await expect(page.getByTestId("recovery")).toHaveCount(0);
          const bad = await layoutProblems(page);
          for (const b of bad) report.push(`${name}: ${b}`);
          await page.screenshot({ path: `${OUT}/${lang}-${scheme}-${w}-${name}.png` });
        }
        // The record sheet, bulk selection and dialogs.
        await page.goto(`/#/${model}/${model}?view=list`);
        await page.reload();
        await page.waitForTimeout(600);
        await page.getByRole("checkbox").first().check();
        await page.waitForTimeout(300);
        await page.screenshot({ path: `${OUT}/${lang}-${scheme}-${w}-bulkbar.png` });
        for (const b of await layoutProblems(page)) report.push(`bulkbar: ${b}`);
        await page
          .locator("td.selcol input")
          .nth(1)
          .click()
          .catch(() => {});
        await page.goto(`/#/${model}/${model}?view=list`);
        await page.reload();
        await page.waitForTimeout(500);
        await page.locator("tbody tr td:nth-child(2)").first().click();
        await page.waitForTimeout(700);
        await page.screenshot({ path: `${OUT}/${lang}-${scheme}-${w}-record.png` });
        for (const b of await layoutProblems(page)) report.push(`record: ${b}`);
        writeFileSync(`${OUT}/${lang}-${scheme}-${w}.txt`, report.join("\n"));
        console.log(`[qa ${lang} ${scheme} ${w}] ${report.length} problems\n${report.join("\n")}`);
      });
    }
  }
}

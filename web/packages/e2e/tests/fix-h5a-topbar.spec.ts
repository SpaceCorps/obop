import { expect, test } from "../support/fixtures.ts";

// The app's name in the top bar of a 320 px phone (docs/ISSUES.md Q18): it takes up to two lines
// instead of an ellipsis, and a long compound breaks at its seams ("Rechnungs-" / "stellung"), so
// no name is cut. A name that two lines still cut (Swedish "E-post-" / "marknads-" / "föring")
// gets a smaller size on three lines. The bar keeps its height.

async function nameFits(page: import("@playwright/test").Page) {
  const name = page.locator(".brand-app");
  await expect(name).toBeVisible();
  return name.evaluate((el) => ({
    text: el.textContent ?? "",
    wide: el.scrollWidth <= el.clientWidth + 1,
    tall: el.scrollHeight <= el.clientHeight + 1,
    bar: (document.querySelector(".topbar") as HTMLElement).getBoundingClientRect().height,
  }));
}

test("a long app name is whole in the top bar at 320 px (Q18)", async ({ page, signIn }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await signIn("root");
  await page.goto("/?lang=de#/invoicing/invoice");
  await expect(page.locator(".brand-app")).toHaveText(
    `Rechnungs${String.fromCharCode(0xad)}stellung`,
  );
  const de = await nameFits(page);
  expect(de.wide && de.tall).toBe(true);
  expect(de.bar).toBeLessThanOrEqual(57);
});

test("every app's name is whole at 320 px in Swedish, Finnish and German (Q18)", async ({
  page,
  signIn,
  api,
}) => {
  test.setTimeout(180_000);
  const root = await api.token("root");
  // The longest name of the catalogue: "E-postmarknadsföring" needs three lines.
  await api.post("/api/marketplace_listing/install", { id: "email_marketing" }, root);
  try {
    await page.setViewportSize({ width: 320, height: 700 });
    await signIn("root");
    const cut: string[] = [];
    for (const lang of ["sv", "fi", "de"]) {
      await page.goto(`/?lang=${lang}#/`);
      await expect(page.locator("a.app-tile").first()).toBeVisible();
      const homes = await page
        .locator("a.app-tile")
        .evaluateAll((as) => [...new Set(as.map((a) => a.getAttribute("href") ?? ""))]);
      expect(homes.length).toBeGreaterThan(3);
      for (const home of homes.filter((h) => h.startsWith("#/"))) {
        await page.goto(`/?lang=${lang}${home}`);
        const r = await nameFits(page);
        if (!(r.wide && r.tall) || r.bar > 57) cut.push(`${lang} ${home}: ${r.text} (${r.bar} px)`);
      }
    }
    expect(cut).toEqual([]);
    // Opened afresh, after the web font arrives, as well as reached from the launcher.
    await page.goto("/?lang=sv#/email_marketing");
    await page.reload();
    await page.evaluate(() => document.fonts.ready);
    const sv = await nameFits(page);
    expect(sv.wide && sv.tall, sv.text).toBe(true);
  } finally {
    await api.post("/api/module/uninstall", { id: "email_marketing" }, root);
  }
});

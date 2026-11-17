import { expect, test } from "../support/fixtures.ts";

// The app's name in the top bar of a 320 px phone (docs/ISSUES.md Q18): it takes up to two lines
// instead of an ellipsis, and a long compound breaks at its seams ("Rechnungs-" / "stellung"), so
// no name is cut. The bar keeps its height.

async function nameFits(page: import("@playwright/test").Page) {
  const name = page.locator(".brand-app");
  await expect(name).toBeVisible();
  return name.evaluate((el) => ({
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

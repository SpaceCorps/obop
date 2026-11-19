import { expect, test } from "../support/fixtures.ts";

// U1 (docs/ISSUES.md): Contacts follows the name order, and the placeholder name of an erased
// contact ("[erased]") sorted before every letter, so the list opened with erased contacts. The
// list now opens with the filter "Not erased"; removing it shows them again.

test("Contacts opens without erased contacts, and the filter can be removed", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const gone = await api.create("root", "party", { name: uid("Aaron Erased") });
  await api.run("root", "party.erase", { id: gone.id });
  const kept = uid("Aaron Kept");
  await api.create("root", "party", { name: kept });

  await signIn("root", "/base/party?view=list");
  await expect(page.getByText(kept).first()).toBeVisible();
  await expect(page.getByText("[erased]")).toHaveCount(0);

  // The filter is a chip the person can remove; then the erased contact is listed again.
  const chip = page.locator(".facet").filter({ hasText: "Not erased" });
  await expect(chip).toBeVisible();
  await chip.getByRole("button", { name: "Remove filter Not erased" }).click();
  await expect(page.getByText("[erased]").first()).toBeVisible();
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  // Every phone visit to Contacts shows the chip, so its remove button takes a tap 44 px wide
  // although it is drawn 32 px wide.
  test("the remove button of the filter chip has a 44 px hit area", async ({ page, signIn }) => {
    await signIn("root", "/base/party?view=list");
    const button = page
      .locator(".facet")
      .filter({ hasText: "Not erased" })
      .getByRole("button", { name: "Remove filter Not erased" });
    await expect(button).toBeVisible();
    const hit = await button.evaluate((b) => {
      const r = b.getBoundingClientRect();
      const y = r.top + r.height / 2;
      const x = r.left + r.width / 2;
      const at = (px: number, py: number) => document.elementFromPoint(px, py)?.closest("button");
      return [at(r.left - 5, y), at(r.right + 5, y), at(x, r.top - 5), at(x, r.bottom + 5)].every(
        (e) => e === b,
      );
    });
    expect(hit).toBe(true);
  });
});

import { expect, test } from "../support/fixtures.ts";

// U84 (docs/ISSUES.md): Enter on "Remove filter …" removed the chip and left the focus on the page
// body, so a keyboard user started again from the top. The focus now moves to the search input of
// the control panel, which every list shares (Contacts opens with the chip Not erased).

test("removing a filter chip with the keyboard keeps the focus in the search", async ({
  page,
  signIn,
}) => {
  await signIn("root", "/base/party?view=list");
  const chip = page.locator(".facet").filter({ hasText: "Not erased" });
  const remove = chip.getByRole("button", { name: "Remove filter Not erased" });
  await expect(remove).toBeVisible();
  await remove.focus();
  await page.keyboard.press("Enter");
  await expect(chip).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: /^Search/ })).toBeFocused();
});

test("removing a grouping chip with the keyboard keeps the focus in the search", async ({
  page,
  signIn,
}) => {
  const st = encodeURIComponent(JSON.stringify({ g: ["state"] }));
  await signIn("root", `/sales/sale_order?view=list&st=${st}`);
  const remove = page.getByRole("button", { name: /^Remove grouping by/ });
  await expect(remove).toBeVisible();
  await remove.focus();
  await page.keyboard.press("Space");
  await expect(remove).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: /^Search/ })).toBeFocused();
});

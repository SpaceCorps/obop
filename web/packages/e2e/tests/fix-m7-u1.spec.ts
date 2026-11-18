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

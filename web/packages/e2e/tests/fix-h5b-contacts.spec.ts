import { expect, test } from "../support/fixtures.ts";

// Erased contacts no longer open the Contacts list (docs/ISSUES.md U1): the list starts with the
// filter "Not erased", which the person removes like any filter. The filter's label is translated
// by the label files of the module (U10), so a German back office reads "Nicht gelöscht".

test("the contact list opens without erased contacts (U1)", async ({ page, signIn, api, uid }) => {
  const root = await api.token("root");
  const kept = await api.create("root", "party", { name: uid("Aaron Kept") });
  const gone = await api.create("root", "party", { name: uid("Zed Gone") });
  const erased = await api.post("/api/party/erase", { id: gone.id }, root);
  expect(erased.status).toBe(200);

  await signIn("root", "/base/party?view=list");
  const grid = page.getByRole("grid");
  await expect(page.locator(".facet")).toHaveText(/Not erased/);
  await expect(grid.getByRole("row", { name: new RegExp(kept.name) })).toBeVisible();
  await expect(grid.getByRole("row", { name: /\[erased\]/ })).toHaveCount(0);
  await page.getByRole("button", { name: /^Remove filter/ }).click();
  await expect(grid.getByRole("row", { name: /\[erased\]/ }).first()).toBeVisible();
});

test.describe("in German", () => {
  test.use({ language: "de" });
  test("the filter a module declares is shown in the language (U10)", async ({ page, signIn }) => {
    await signIn("root", "/base/party?view=list");
    await expect(page.locator(".facet")).toHaveText(/Nicht gelöscht/);
  });
});

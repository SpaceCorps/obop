import { expect, test } from "../support/fixtures.ts";
import { tr } from "../support/catalogue.ts";

// A message on the screen is worded again after a switch of the language (docs/ISSUES.md D9): the
// sign-in screen keeps how to word its refusal, not the English sentence it made first.

test("the refusal of a sign-in is worded again in the new language (D9)", async ({
  page,
  allowHttpErrors,
}) => {
  allowHttpErrors(401);
  await page.goto("/");
  await page.getByLabel("Username").fill("nobody");
  await page.getByLabel("Password").fill("not a real password");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toHaveText("Invalid username or password");

  await page.getByLabel("Language").selectOption("de");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await expect(alert).toHaveText(tr("de", "Invalid username or password"));
});

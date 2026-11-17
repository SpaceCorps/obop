import { expect, test } from "../support/fixtures.ts";
import { users } from "../support/env.ts";

// The login the back office keeps after a password sign-in (docs/ISSUES.md U11) is the one the
// server signed in: `$user` in a filter or a read-only hint compares with it. Typed with a capital
// letter and a space, it is still the login, as the server reads it.

test("a login typed with a capital and a space is kept as the server reads it (U11)", async ({
  page,
}) => {
  await page.goto("/");
  const typed = ` ${users.sam.name.charAt(0).toUpperCase()}${users.sam.name.slice(1)}`;
  await page.getByLabel("Username").fill(typed);
  await page.getByLabel("Password").fill(users.sam.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.locator(".topbar")).toBeVisible();
  expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe(users.sam.name);
});

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

test("the login kept is the one the session names, not a guess from the typed text (U11)", async ({
  page,
}) => {
  // The session's own answer wins over anything the page could work out from what was typed:
  // here the two-factor status names the login in a form no rule of the page would produce.
  const named = `${users.sam.name}-as-named-by-the-server`;
  let asked = false;
  await page.route("**/api/auth/mfa", async (route) => {
    if (asked || route.request().method() !== "GET") return route.fallback();
    asked = true;
    const res = await route.fetch();
    const json = (await res.json()) as Record<string, unknown>;
    await route.fulfill({ response: res, json: { ...json, user: named } });
  });
  await page.goto("/");
  await page.getByLabel("Username").fill(` ${users.sam.name.toUpperCase()} `);
  await page.getByLabel("Password").fill(users.sam.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.locator(".topbar")).toBeVisible();
  expect(asked).toBe(true);
  expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe(named);
});

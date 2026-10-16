import { users } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

test.describe("sign in and out", () => {
  test("a valid username and password open the launcher home", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "obop" })).toBeVisible();
    await page.getByLabel("Username").fill(users.root.name);
    await page.getByLabel("Password").fill(users.root.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Contacts" })).toBeVisible();
    await expect(page.getByRole("button", { name: "R", exact: true })).toBeVisible();
  });

  test("a wrong password is refused with a clear message and no session", async ({
    page,
    allowHttpErrors,
  }) => {
    allowHttpErrors(401);
    await page.goto("/");
    await page.getByLabel("Username").fill("nobody");
    await page.getByLabel("Password").fill("not a real password");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("alert")).toHaveText("Invalid username or password");
    await expect(page.locator(".topbar")).toHaveCount(0);
    expect(await page.evaluate(() => sessionStorage.getItem("obop-token"))).toBeNull();
  });

  test("signing out returns to the form and a reload stays signed out", async ({ page }) => {
    await page.goto("/");
    await page.getByLabel("Username").fill(users.sam.name);
    await page.getByLabel("Password").fill(users.sam.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    // The session survives a reload...
    await page.reload();
    await expect(page.locator(".topbar")).toBeVisible();
    // ...until the person signs out.
    await page.getByRole("button", { name: "Account" }).click();
    await expect(page.getByText(users.sam.name)).toBeVisible();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);
  });
});

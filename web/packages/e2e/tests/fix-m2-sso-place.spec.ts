// The place a deep link is kept for during the round trip to Microsoft (D91), in a real browser:
// a failed return forgets it, so a later password sign-in lands on the launcher and not on the
// place of a sign-in that did not happen; and a return that a tab with a session receives is
// dropped, not shown after a later sign-out (docs/ISSUES.md N20 c). The stand-in is not Entra
// (support/idp.ts); nothing here talks to Microsoft.
import { idpUrl, sso, ssoUrl } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

const FAILED = /The sign-in with Microsoft could not be completed\./;
const PLACE = "obop-sso-place";

test.describe("the place kept for a Microsoft sign-in", () => {
  async function atMicrosoft(page: import("@playwright/test").Page, who: string) {
    await expect(page).toHaveURL(new RegExp(`^${idpUrl()}/`));
    await page
      .getByRole("link", { name: who === "cancel" ? "Cancel" : `Sign in as ${who}` })
      .click();
  }

  async function signInWithPassword(page: import("@playwright/test").Page) {
    await page.getByLabel("Username").fill(sso.users.bea.name);
    await page.getByLabel("Password").fill(sso.users.bea.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.locator(".topbar")).toBeVisible();
  }

  test("a return with an error forgets the place", async ({ page }) => {
    await page.goto(`${ssoUrl()}/#/crm/lead`);
    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "stranger");
    await expect(page.getByText(FAILED)).toBeVisible();
    expect(await page.evaluate((k) => sessionStorage.getItem(k), PLACE)).toBeNull();
    await signInWithPassword(page);
    expect(new URL(page.url()).hash, "the launcher, not the place of the failed sign-in").not.toBe(
      "#/crm/lead",
    );
  });

  test("an exchange that fails forgets the place", async ({ page, allowHttpErrors }) => {
    allowHttpErrors(400);
    // The one-time code comes back, and the exchange is answered as a refusal.
    await page.route("**/api/auth/oidc/exchange", (route) =>
      route.fulfill({
        status: 400,
        contentType: "application/json",
        body: JSON.stringify({ error: "the code is not valid" }),
      }),
    );
    await page.goto(`${ssoUrl()}/#/crm/lead`);
    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "bea");
    await expect(page.getByText(FAILED)).toBeVisible();
    expect(page.url()).toBe(`${ssoUrl()}/`);
    expect(await page.evaluate((k) => sessionStorage.getItem(k), PLACE)).toBeNull();
    await page.unroute("**/api/auth/oidc/exchange");
    await signInWithPassword(page);
    expect(new URL(page.url()).hash).not.toBe("#/crm/lead");
  });

  test("a return that reaches a tab with a session is dropped, also after a sign-out", async ({
    page,
  }) => {
    await page.goto(ssoUrl());
    await signInWithPassword(page);
    // The browser comes back from Microsoft into the tab that is signed in already (a new load
    // of the page, as the redirect of the callback is).
    await page.goto("about:blank");
    await page.goto(`${ssoUrl()}/#oidc_error=failed`);
    await expect(page.locator(".topbar")).toBeVisible();
    expect(new URL(page.url()).hash, "the fragment is cleaned at once").not.toContain("oidc");
    await expect(page.getByText(FAILED)).toHaveCount(0);
    // After a sign-out in the same page the sign-in page shows no notice that nobody caused.
    await page.getByRole("button", { name: "Account" }).click();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await expect(page.getByRole("button", { name: "Sign in with Microsoft" })).toBeVisible();
    await expect(page.getByText(FAILED)).toHaveCount(0);
  });
});

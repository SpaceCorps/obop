// The administrator's screen for Sign in with Microsoft (D91), all through the page: set the
// provider up in the form, see the button on the sign-in page, bind a person by object id in People,
// sign in as that person with the button, take it all away again. The stand-in for Microsoft Entra
// is the one of tests/sso.spec.ts (support/idp.ts); this spec has an instance of its own (`ssoadmin`:
// the same accounts, nothing set up), so it does not touch the one that spec shares.
import type { Page } from "@playwright/test";
import { idpUrl, sso, ssoAdminUrl } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

const PLACE = "/customize/security?tab=security";
const BEA = sso.identities.bea.oid;

test.describe.configure({ mode: "serial" });

/** The administrator, signed in with the API (not under test) and the page opened on Security. */
async function adminPage(page: Page, request: import("@playwright/test").APIRequestContext) {
  const login = await request.post(`${ssoAdminUrl()}/api/auth/login`, {
    data: { username: sso.users.root1.name, password: sso.users.root1.password },
  });
  expect(login.status()).toBe(200);
  const token = ((await login.json()) as { token: string }).token;
  await page.addInitScript(
    ([t, u]) => {
      sessionStorage.setItem("obop-token", t!);
      sessionStorage.setItem("obop-user", u!);
    },
    [token, sso.users.root1.name],
  );
  await page.goto(`${ssoAdminUrl()}/#${PLACE}`);
  await expect(page.getByRole("heading", { name: "Sign in with Microsoft" })).toBeVisible();
}

const section = (page: Page) =>
  page.locator("section").filter({ has: page.getByRole("heading", { name: "Sign in with Microsoft" }) });
const row = (page: Page, name: string) =>
  page
    .getByRole("table", { name: "People" })
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name, exact: true }) });

/** Whether a stranger's sign-in page offers the button. */
async function offersButton(browser: import("@playwright/test").Browser) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await page.goto(ssoAdminUrl());
  await expect(page.getByLabel("Username")).toBeVisible();
  const n = await page.getByRole("button", { name: "Sign in with Microsoft" }).count();
  await ctx.close();
  return n > 0;
}

test.describe("administering Sign in with Microsoft", () => {
  test("nothing is set up: one button, no column in People, no button on the sign-in page", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    const s = section(page);
    await expect(s.getByText("Not set up")).toBeVisible();
    await expect(s.getByRole("button")).toHaveCount(1);
    await expect(s.getByRole("button", { name: "Set up Microsoft sign-in" })).toBeVisible();
    await expect(page.getByRole("columnheader", { name: "Microsoft" })).toHaveCount(0);
    await expect(row(page, "bea").getByRole("button", { name: "Microsoft" })).toHaveCount(0);
    expect(await offersButton(browser)).toBe(false);
  });

  test("the form refuses what is not a GUID, then sets Microsoft up and the button appears", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    await section(page).getByRole("button", { name: "Set up Microsoft sign-in" }).click();
    const form = page.getByRole("dialog", { name: "Set up Microsoft sign-in" });
    await expect(form.getByLabel("Redirect address")).toHaveValue(
      `${ssoAdminUrl()}/api/auth/oidc/callback`,
    );
    await expect(form.getByLabel("Client secret")).toHaveAttribute("type", "password");

    await form.getByLabel("Tenant ID").fill("common");
    await form.getByLabel("Application (client) ID").fill("nope");
    await form.getByRole("button", { name: "Save" }).click();
    await expect(form.getByText("The tenant ID is a GUID.")).toBeVisible();
    await expect(form.getByText("The application ID is a GUID.")).toBeVisible();
    await expect(form.getByText("Enter the client secret.")).toBeVisible();

    await form.getByLabel("Tenant ID").fill(sso.tenant);
    await form.getByLabel("Application (client) ID").fill(sso.clientId);
    await form.getByLabel("Client secret").fill(sso.clientSecret);
    await form.getByRole("button", { name: "Save" }).click();
    await expect(form).toHaveCount(0);

    const s = section(page);
    await expect(s.getByText("On", { exact: true })).toBeVisible();
    await expect(s.getByText(sso.tenant)).toBeVisible();
    await expect(s.getByText(sso.clientId)).toBeVisible();
    await expect(s.getByText("Stored")).toBeVisible();
    // The secret is nowhere on the page, also not after a reload.
    await page.reload();
    await expect(s.getByText(sso.tenant)).toBeVisible();
    expect(await page.content()).not.toContain(sso.clientSecret);
    // The column and the button of People are there now, and the sign-in page offers the button.
    await expect(page.getByRole("columnheader", { name: "Microsoft" })).toBeVisible();
    await expect(row(page, "bea").getByText("Not bound")).toBeVisible();
    expect(await offersButton(browser)).toBe(true);
  });

  test("on a phone: the section, the People row and the dialog fit the screen", async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await adminPage(page, request);
    expect(await layoutProblems(page)).toEqual([]);
    await section(page).getByRole("button", { name: "Change" }).click();
    const form = page.getByRole("dialog", { name: "Change Microsoft sign-in" });
    await expect(form.getByText("Leave empty to keep the stored secret.")).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    await form.getByRole("button", { name: "Cancel" }).click();
    const bea = row(page, "bea");
    const button = bea.getByRole("button", { name: "Microsoft", exact: true });
    await button.scrollIntoViewIfNeeded();
    expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await button.click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await expect(dialog.getByLabel("Object ID")).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    await dialog.getByRole("button", { name: "Cancel" }).click();
    // The keyboard goes back to the button that opened the dialog.
    await expect(button).toBeFocused();
  });

  test("bind bea by object id, and she signs in with the Microsoft button", async ({
    page,
    request,
    browser,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await adminPage(page, request);
    await row(page, "bea").getByRole("button", { name: "Microsoft", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    // An e-mail address is refused before anything is sent.
    await dialog.getByLabel("Object ID").fill("bea@example.test");
    await dialog.getByRole("button", { name: "Bind" }).click();
    await expect(dialog.getByText("The object ID is a GUID, not an e-mail address.")).toBeVisible();
    await dialog.getByLabel("Object ID").fill(BEA);
    await dialog.getByRole("button", { name: "Bind" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row(page, "bea").getByText("Bound", { exact: true })).toBeVisible();

    // The same object id for a second person is refused with the server's own sentence.
    await row(page, "ada").getByRole("button", { name: "Microsoft", exact: true }).click();
    const again = page.getByRole("dialog", { name: "Microsoft sign-in for ada" });
    await again.getByLabel("Object ID").fill(BEA);
    await again.getByRole("button", { name: "Bind" }).click();
    await expect(again.getByRole("alert")).toContainText("bound to another account already");
    await again.getByRole("button", { name: "Cancel" }).click();

    // Bea signs in with the button, in a browser of her own.
    const ctx = await browser.newContext();
    const her = await ctx.newPage();
    await her.goto(ssoAdminUrl());
    await her.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await expect(her).toHaveURL(new RegExp(`^${idpUrl()}/`));
    await her.getByRole("link", { name: "Sign in as bea" }).click();
    await expect(her.locator(".topbar")).toBeVisible();
    expect(await her.evaluate(() => sessionStorage.getItem("obop-user"))).toBe("bea");
    await ctx.close();
  });

  test("Microsoft only: the server's rule is shown, and the binding stays while it is on or refused", async ({
    page,
    request,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await adminPage(page, request);
    await row(page, "bea").getByRole("button", { name: "Microsoft", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await expect(dialog.getByText(`Bound to ${BEA}`)).toBeVisible();
    // Nobody here keeps a password and a second step, so the server refuses, and says why.
    await dialog.getByRole("checkbox", { name: /Microsoft only/ }).check({ force: true });
    await expect(dialog.getByRole("alert")).toContainText("other enabled administrators");
    await expect(dialog.getByRole("checkbox", { name: /Microsoft only/ })).not.toBeChecked();
  });

  test("the switch hides and shows the button, and the binding is removed through the page", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    const s = section(page);
    await s.getByRole("switch", { name: "Show the button on the sign-in page" }).uncheck();
    await expect(s.getByText("Off", { exact: true })).toBeVisible();
    expect(await offersButton(browser)).toBe(false);
    await s.getByRole("switch", { name: "Show the button on the sign-in page" }).check();
    await expect(s.getByText("On", { exact: true })).toBeVisible();
    expect(await offersButton(browser)).toBe(true);

    await row(page, "bea").getByRole("button", { name: "Microsoft", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await dialog.getByRole("button", { name: "Remove binding" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row(page, "bea").getByText("Not bound")).toBeVisible();
  });

  test("Remove asks first, then the configuration, the column and the button are gone", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    await section(page).getByRole("button", { name: "Remove", exact: true }).click();
    const ask = page.getByRole("alertdialog", { name: "Remove Microsoft sign-in?" });
    await ask.getByRole("button", { name: "Cancel" }).click();
    await expect(section(page).getByText("On", { exact: true })).toBeVisible();

    await section(page).getByRole("button", { name: "Remove", exact: true }).click();
    await page
      .getByRole("alertdialog", { name: "Remove Microsoft sign-in?" })
      .getByRole("button", { name: "Remove", exact: true })
      .click();
    await expect(section(page).getByText("Not set up")).toBeVisible();
    await expect(page.getByRole("columnheader", { name: "Microsoft" })).toHaveCount(0);
    expect(await offersButton(browser)).toBe(false);
  });
});

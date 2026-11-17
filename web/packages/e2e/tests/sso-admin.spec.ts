// The administrator's screen for Sign in with Microsoft (D91), all through the page: set the
// provider up in the form, see the button on the sign-in page, bind a person by object id in People,
// sign in as that person with the button, take it all away again. The stand-in for Microsoft Entra
// is the one of tests/sso.spec.ts (support/idp.ts); this spec has an instance of its own (`ssoadmin`:
// the same accounts, nothing set up), so it does not touch the one that spec shares.
import type { Page } from "@playwright/test";
import { idpUrl, sso, ssoAdminUrl } from "../support/env.ts";
import { tr } from "../support/catalogue.ts";
import { expect, test, unnamedControls } from "../support/fixtures.ts";
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

const section = (page: Page, name = "Sign in with Microsoft") => page.getByRole("region", { name });
/** The "Microsoft" button of one person's row; its name carries the person. */
const microsoft = (page: Page, person: string) =>
  row(page, person).getByRole("button", { name: `Microsoft sign-in for ${person}`, exact: true });
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
    await expect(row(page, "bea").getByRole("button", { name: /^Microsoft/ })).toHaveCount(0);
    expect(await offersButton(browser)).toBe(false);
  });

  test("the form refuses what is not a GUID, then sets Microsoft up and the button appears", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    const setUp = section(page).getByRole("button", { name: "Set up Microsoft sign-in" });
    await setUp.click();
    const form = page.getByRole("dialog", { name: "Set up Microsoft sign-in" });
    // Escape closes the form, and the keyboard is back on the button that opened it.
    await page.keyboard.press("Escape");
    await expect(form).toHaveCount(0);
    await expect(setUp).toBeFocused();
    await setUp.click();
    await expect(form.getByLabel("Redirect address", { exact: true })).toHaveValue(
      `${ssoAdminUrl()}/api/auth/oidc/callback`,
    );
    await expect(form.getByLabel("Client secret")).toHaveAttribute("type", "password");
    // A password manager must not take the field for a login: "off" is ignored, "new-password" is not.
    await expect(form.getByLabel("Client secret")).toHaveAttribute("autocomplete", "new-password");
    expect(await unnamedControls(page), "set-up form").toEqual([]);

    await form.getByLabel("Tenant ID").fill("common");
    await form.getByLabel("Application (client) ID").fill("nope");
    await form.getByRole("button", { name: "Save" }).click();
    await expect(form.getByText("The tenant ID is a GUID.")).toBeVisible();
    await expect(form.getByText("The application ID is a GUID.")).toBeVisible();
    await expect(form.getByText("Enter the client secret.")).toBeVisible();
    // The keyboard goes to the first wrong field, which names its error.
    await expect(form.getByLabel("Tenant ID")).toBeFocused();
    await expect(form.getByLabel("Tenant ID")).toHaveAccessibleDescription(
      /The tenant ID is a GUID\./,
    );

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
    // The button that opened the form is gone: the heading has the keyboard.
    await expect(s.getByRole("heading", { name: "Sign in with Microsoft" })).toBeFocused();
    // The column and the button of People are there at once, with no reload.
    await expect(page.getByRole("columnheader", { name: "Microsoft" })).toBeVisible();
    await expect(row(page, "bea").getByText("Not bound")).toBeVisible();
    // The secret is nowhere on the page, also not after a reload.
    await page.reload();
    await expect(s.getByText(sso.tenant)).toBeVisible();
    expect(await page.content()).not.toContain(sso.clientSecret);
    expect(await offersButton(browser)).toBe(true);

    // The same screen in German reads from the German catalogue, the section and People's button.
    await page.goto(`${ssoAdminUrl()}/?lang=de#${PLACE}`);
    const de = section(page, tr("de", "Sign in with Microsoft"));
    await expect(de).toBeVisible();
    await expect(
      de.getByRole("button", { name: tr("de", "Change settings"), exact: true }),
    ).toBeVisible();
    await expect(de.getByText(tr("de", "Stored"))).toBeVisible();
    await expect(
      de.getByRole("switch", { name: tr("de", "Show the button on the sign-in page") }),
    ).toBeVisible();
    await expect(
      page
        .getByRole("table", { name: tr("de", "People") })
        .getByRole("row")
        .filter({ has: page.getByRole("cell", { name: "bea", exact: true }) })
        .getByRole("button", {
          name: tr("de", "Microsoft sign-in for {user}").replace("{user}", "bea"),
          exact: true,
        }),
    ).toBeVisible();
    await de.getByRole("button", { name: tr("de", "Change settings"), exact: true }).click();
    const deForm = page.getByRole("dialog", { name: tr("de", "Change Microsoft sign-in") });
    await expect(deForm.getByLabel(tr("de", "Client secret"))).toBeVisible();
    await expect(
      deForm.getByRole("button", { name: tr("de", "Copy redirect address") }),
    ).toBeVisible();
    await deForm.getByRole("button", { name: tr("de", "Cancel") }).click();
  });

  test("on a phone: the section, the People row and the dialog fit the screen", async ({
    page,
    request,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await adminPage(page, request);
    expect(await layoutProblems(page)).toEqual([]);
    const change = section(page).getByRole("button", { name: "Change settings", exact: true });
    await change.click();
    const form = page.getByRole("dialog", { name: "Change Microsoft sign-in" });
    await expect(form.getByText("Leave empty to keep the stored secret.")).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    await form.getByRole("button", { name: "Cancel" }).click();
    await expect(change).toBeFocused();
    const button = microsoft(page, "bea");
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
    await microsoft(page, "bea").click();
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
    await microsoft(page, "ada").click();
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
    await microsoft(page, "bea").click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await expect(dialog.getByText(`Bound to ${BEA}`)).toBeVisible();
    expect(await unnamedControls(page), "People Microsoft dialog").toEqual([]);
    // The question comes first, and Cancel sends nothing.
    await dialog.getByRole("checkbox", { name: /Microsoft only/ }).click({ force: true });
    const ask = page.getByRole("alertdialog", { name: "Make bea sign in with Microsoft only?" });
    await expect(ask).toContainText("Their sessions and phone sign-ins end.");
    expect(await unnamedControls(page), "Microsoft only question").toEqual([]);
    await ask.getByRole("button", { name: "Cancel" }).click();
    await expect(ask).toHaveCount(0);
    await expect(dialog.getByRole("alert")).toHaveCount(0);
    // Nobody here keeps a password and a second step, so the server refuses, and says why.
    await dialog.getByRole("checkbox", { name: /Microsoft only/ }).click({ force: true });
    await ask.getByRole("button", { name: "Make Microsoft only" }).click();
    await expect(dialog.getByRole("alert")).toContainText("other enabled administrators");
    await expect(dialog.getByRole("checkbox", { name: /Microsoft only/ })).not.toBeChecked();
    // The keyboard goes back to the row's button when the dialog is closed.
    await dialog.locator("button.btn", { hasText: "Close" }).click();
    await expect(microsoft(page, "bea")).toBeFocused();
  });

  test("the switch hides and shows the button, and the binding is removed through the page", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    const s = section(page);
    const toggle = s.getByRole("switch", { name: "Show the button on the sign-in page" });
    // The switch follows the server's answer, so it is clicked and then read, not "unchecked".
    await toggle.click();
    await expect(toggle).not.toBeChecked();
    await expect(s.getByText("Off", { exact: true })).toBeVisible();
    expect(await offersButton(browser)).toBe(false);
    await toggle.click();
    await expect(toggle).toBeChecked();
    await expect(s.getByText("On", { exact: true })).toBeVisible();
    expect(await offersButton(browser)).toBe(true);

    await microsoft(page, "bea").click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await dialog.getByRole("button", { name: "Remove binding" }).click();
    const ask = page.getByRole("alertdialog", { name: "Remove the Microsoft binding of bea?" });
    await expect(ask).toContainText("Their sessions and phone sign-ins end.");
    await ask.getByRole("button", { name: "Cancel" }).click();
    await expect(ask).toHaveCount(0);
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Remove binding" }).click();
    await ask.getByRole("button", { name: "Remove binding" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(row(page, "bea").getByText("Not bound")).toBeVisible();
  });

  test("Remove asks first, then the configuration, the column and the button are gone", async ({
    page,
    request,
    browser,
  }) => {
    await adminPage(page, request);
    const remove = section(page).getByRole("button", { name: "Remove", exact: true });
    await remove.click();
    const ask = page.getByRole("alertdialog", { name: "Remove Microsoft sign-in?" });
    expect(await unnamedControls(page), "Remove question").toEqual([]);
    await ask.getByRole("button", { name: "Cancel" }).click();
    await expect(section(page).getByText("On", { exact: true })).toBeVisible();
    await expect(remove).toBeFocused();

    await section(page).getByRole("button", { name: "Remove", exact: true }).click();
    await page
      .getByRole("alertdialog", { name: "Remove Microsoft sign-in?" })
      .getByRole("button", { name: "Remove", exact: true })
      .click();
    await expect(section(page).getByText("Not set up")).toBeVisible();
    // The Remove button is gone with the configuration: the heading has the keyboard.
    await expect(
      section(page).getByRole("heading", { name: "Sign in with Microsoft" }),
    ).toBeFocused();
    await expect(page.getByRole("columnheader", { name: "Microsoft" })).toHaveCount(0);
    expect(await offersButton(browser)).toBe(false);
  });
});

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
/** The "Microsoft" cell of one person's row is the button; its name has the person and the state. */
const microsoft = (page: Page, person: string) =>
  row(page, person).getByRole("button", { name: new RegExp(`^Microsoft sign-in for ${person}: `) });
const row = (page: Page, name: string) =>
  page
    .getByRole("table", { name: "People" })
    .getByRole("row")
    .filter({ has: page.getByRole("cell", { name, exact: true }) });

/** Whether a stranger's sign-in page offers the button. */
async function offersButton(browser: import("@playwright/test").Browser) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  // The page asks /api/auth/sso after it has drawn the form: count only once the server answered
  // (a 429 is asked again later, N19), or a slow answer reads as "no button".
  const answered = page.waitForResponse(
    (r) => new URL(r.url()).pathname === "/api/auth/sso" && r.status() !== 429,
  );
  await page.goto(ssoAdminUrl());
  await expect(page.getByLabel("Username")).toBeVisible();
  const enabled = ((await (await answered).json()) as { enabled?: boolean }).enabled === true;
  const button = page.getByRole("button", { name: "Sign in with Microsoft" });
  if (enabled) await expect(button).toBeVisible();
  const n = await button.count();
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
    await page.setViewportSize({ width: 1280, height: 800 });
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
    // On a laptop the Copy button is beside the address, on the same row.
    const address = await form.getByLabel("Redirect address", { exact: true }).boundingBox();
    const copy = await form.getByRole("button", { name: "Copy redirect address" }).boundingBox();
    expect(copy!.x).toBeGreaterThanOrEqual(address!.x + address!.width - 1);
    expect(copy!.y + copy!.height).toBeGreaterThan(address!.y);
    expect(copy!.y).toBeLessThan(address!.y + address!.height);
    await expect(form.getByLabel("Client secret")).toHaveAttribute("type", "password");
    // A password manager must not take the field for a login: "off" is ignored, "new-password" is not.
    await expect(form.getByLabel("Client secret")).toHaveAttribute("autocomplete", "new-password");
    expect(await unnamedControls(page), "set-up form").toEqual([]);

    await form.getByLabel("Tenant ID").fill("common");
    await form.getByLabel("Application (client) ID").fill("nope");
    await form.getByRole("button", { name: "Save" }).click();
    await expect(
      form.getByText("This is not a tenant ID: it has 32 letters and digits in five groups."),
    ).toBeVisible();
    await expect(
      form.getByText("This is not an application ID: it has 32 letters and digits in five groups."),
    ).toBeVisible();
    await expect(form.getByText("Enter the client secret.")).toBeVisible();
    // The keyboard goes to the first wrong field, which names its error.
    await expect(form.getByLabel("Tenant ID")).toBeFocused();
    await expect(form.getByLabel("Tenant ID")).toHaveAccessibleDescription(
      /This is not a tenant ID: it has 32 letters and digits in five groups\./,
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
    await expect(microsoft(page, "bea")).toHaveText("Not bound");
    // On a laptop the People table fits its box (the cell is the control: no second button).
    const widths = await page.getByRole("table", { name: "People" }).evaluate((table) => {
      const box = table.closest<HTMLElement>(".table-wrap") ?? table.parentElement!;
      return {
        table: table.getBoundingClientRect().width,
        scrollWidth: box.scrollWidth,
        clientWidth: box.clientWidth,
      };
    });
    console.log(
      `People table at 1280 px: table ${widths.table}, box scrollWidth ${widths.scrollWidth}, clientWidth ${widths.clientWidth}`,
    );
    expect(widths.scrollWidth).toBeLessThanOrEqual(widths.clientWidth);
    // Change settings and Remove are side by side at the start of the line.
    const change = await s
      .getByRole("button", { name: "Change settings", exact: true })
      .boundingBox();
    const removeBox = await s.getByRole("button", { name: "Remove", exact: true }).boundingBox();
    expect(removeBox!.x - (change!.x + change!.width)).toBeLessThan(40);
    expect(removeBox!.x).toBeGreaterThan(change!.x);
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
          name: tr("de", "Microsoft sign-in for {user}: not bound").replace("{user}", "bea"),
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
    allowHttpErrors,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await adminPage(page, request);
    expect(await layoutProblems(page)).toEqual([]);
    const change = section(page).getByRole("button", { name: "Change settings", exact: true });
    await change.click();
    const form = page.getByRole("dialog", { name: "Change Microsoft sign-in" });
    await expect(form.getByText("Leave empty to keep the stored secret.")).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    // The address is read in full (it wraps), and Copy is under it, flush left, before the hint.
    const field = form.getByLabel("Redirect address", { exact: true });
    const fieldBox = (await field.boundingBox())!;
    const copyBox = (await form
      .getByRole("button", { name: "Copy redirect address" })
      .boundingBox())!;
    expect(copyBox.y).toBeGreaterThanOrEqual(fieldBox.y + fieldBox.height - 1);
    expect(Math.abs(copyBox.x - fieldBox.x)).toBeLessThan(2);
    expect(copyBox.height).toBeGreaterThanOrEqual(44);
    const hintBox = (await form
      .getByText("Register this as a Web redirect address")
      .boundingBox())!;
    expect(hintBox.y).toBeGreaterThanOrEqual(copyBox.y + copyBox.height - 1);
    expect(
      await field.evaluate(
        (el) => el.scrollHeight <= el.clientHeight + 1 && el.scrollWidth <= el.clientWidth + 1,
      ),
    ).toBe(true);
    // A refused Save: the red line takes the focus and lies inside the visible part of the sheet
    // and of the screen (it was drawn under the fold, and nothing happened for the person).
    allowHttpErrors(503);
    // (The page adds the language to the address as a query.)
    const configure = /\/api\/sso_provider\/configure(\?|$)/;
    await page.route(configure, (r) =>
      r.fulfill({ status: 503, contentType: "application/json", body: "{}" }),
    );
    await form.getByRole("button", { name: "Save" }).click();
    const refusal = form.getByRole("alert");
    await expect(refusal).toBeFocused();
    await expect(refusal).toBeInViewport();
    const alertBox = (await refusal.boundingBox())!;
    const bodyBox = (await form.locator(".dialog-body").boundingBox())!;
    expect(alertBox.y).toBeGreaterThanOrEqual(bodyBox.y - 1);
    expect(alertBox.y + alertBox.height).toBeLessThanOrEqual(bodyBox.y + bodyBox.height + 1);
    expect(alertBox.y + alertBox.height).toBeLessThanOrEqual(844);
    await page.unroute(configure);
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
    await expect(dialog.getByText("This is an e-mail address, not an object ID.")).toBeVisible();
    await dialog.getByLabel("Object ID").fill(BEA);
    await dialog.getByRole("button", { name: "Bind" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(microsoft(page, "bea")).toHaveText("Bound");

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

  test("Microsoft only: when the rule is not met the checkbox is off limits and says why", async ({
    page,
    request,
  }) => {
    await adminPage(page, request);
    await microsoft(page, "bea").click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for bea" });
    await expect(dialog.getByText(`Bound to ${BEA}`)).toBeVisible();
    expect(await unnamedControls(page), "People Microsoft dialog").toEqual([]);
    // Nobody here keeps a password and a second step, so the page says so instead of letting the
    // person press a box the server will refuse.
    const box = dialog.getByRole("checkbox", { name: /Microsoft only/ });
    await expect(box).toBeDisabled();
    await expect(box).toHaveAccessibleName(/At least two other administrators with a password/);
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
    await expect(microsoft(page, "bea")).toHaveText("Not bound");
  });

  test("you bind yourself, remove your own binding through the question, and land on the sign-in page", async ({
    page,
    request,
  }) => {
    await adminPage(page, request);
    await microsoft(page, "root1").click();
    const dialog = page.getByRole("dialog", { name: "Microsoft sign-in for root1" });
    await dialog.getByLabel("Object ID").fill("0e0e0e0e-0005-4000-8000-000000000005");
    await dialog.getByRole("button", { name: "Bind" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(microsoft(page, "root1")).toHaveText("Bound");

    await microsoft(page, "root1").click();
    await dialog.getByRole("button", { name: "Remove binding" }).click();
    const ask = page.getByRole("alertdialog", { name: "Remove the Microsoft binding of root1?" });
    await expect(ask).toContainText("This is your own account: you are signed out at once.");
    await ask.getByRole("button", { name: "Remove binding" }).click();
    // The server ended this session: the page is the sign-in page, not a table with nobody in it.
    await expect(page.getByLabel("Username")).toBeVisible();
    await expect(page.getByRole("table", { name: "People" })).toHaveCount(0);
    expect(await page.evaluate(() => sessionStorage.getItem("obop-token") ?? "")).toBe("");
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

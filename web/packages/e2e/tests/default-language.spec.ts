import { request as playwrightRequest, type Page } from "@playwright/test";
import { tr } from "../support/catalogue.ts";
import { languageUrl, users, type UserName } from "../support/env.ts";
import { Api, expect, test } from "../support/fixtures.ts";

// The instance default language (OBOP_DEFAULT_LANGUAGE) reaches the screens that come before
// sign-in, where the instance settings cannot be read yet. A third instance is started with
// OBOP_DEFAULT_LANGUAGE=sv (support/serve.ts, "language"); every spec here opens it in a browser
// that asks for English and has never been there: no saved choice (`language: null` leaves the
// pinned English out), locale en-US, Accept-Language en.
test.use({ language: null, locale: "en-US", extraHTTPHeaders: { "Accept-Language": "en" } });

const META = 'meta[name="obop-default-language"]';

/** Fill the sign-in form in `lang` (its labels come from the catalogue) and submit it. */
async function signInForm(page: Page, lang: string, user: UserName) {
  await page.getByLabel(tr(lang, "Username")).fill(users[user].name);
  await page.getByLabel(tr(lang, "Password")).fill(users[user].password);
  await page.getByRole("button", { name: tr(lang, "Sign in"), exact: true }).click();
  await expect(page.locator(".topbar")).toBeVisible();
}

/** The sign-in page of a signed-out browser: the token lives in the session, the choice in storage. */
async function signOut(page: Page) {
  await page.evaluate(() => sessionStorage.clear());
  await page.reload();
}

const savedChoice = (page: Page) => page.evaluate(() => localStorage.getItem("obop-lang"));

test.describe("the default language of an instance", () => {
  test("serves the sign-in page in Swedish to an English browser", async ({ page, request }) => {
    // The server puts the tag into the document itself, once, and the shell is never cached blindly.
    const res = await request.get(`${languageUrl()}/`, { headers: { "Accept-Language": "en" } });
    expect(res.status()).toBe(200);
    expect(res.headers()["cache-control"]).toBe("no-cache");
    const html = await res.text();
    expect(html.match(/<meta name="obop-default-language" content="sv">/g)).toHaveLength(1);
    expect(html.match(/obop-default-language/g)).toHaveLength(1);
    // A route the app resolves on its own gets the same document.
    const deep = await request.get(`${languageUrl()}/customize/theme`);
    expect(await deep.text()).toContain('content="sv"');

    await page.goto(`${languageUrl()}/`);
    await expect(page.locator("html")).toHaveAttribute("lang", "sv");
    await expect(page.locator(META)).toHaveAttribute("content", "sv");
    // The strings of the sign-in screen are the Swedish catalogue's, not English.
    await expect(
      page.getByRole("heading", { name: tr("sv", "Sign in"), exact: true }),
    ).toBeVisible();
    await expect(page.getByText(tr("sv", "Sign in to your instance."))).toBeVisible();
    await expect(page.getByText(tr("sv", "One business, one platform."))).toBeVisible();
    await expect(page.getByLabel(tr("sv", "Username"))).toBeVisible();
    await expect(page.getByLabel(tr("sv", "Password"))).toBeVisible();
    await expect(
      page.getByRole("button", { name: tr("sv", "Sign in"), exact: true }),
    ).toBeVisible();
    await expect(page.getByLabel(tr("sv", "Language"))).toHaveValue("sv");
    await expect(page.getByRole("heading", { name: "Sign in", exact: true })).toHaveCount(0);
    await expect(page.getByText("Sign in to your instance.")).toHaveCount(0);
    // A default is not a choice: nothing is saved as the person's own.
    expect(await savedChoice(page)).toBeNull();
  });

  test("stays the language of the app after sign-in, until the person chooses another", async ({
    page,
  }) => {
    await page.goto(`${languageUrl()}/`);
    await signInForm(page, "sv", "sam");
    // The settings of this instance name no language, so the server's default carries on.
    await expect(page.locator("html")).toHaveAttribute("lang", "sv");
    await expect(page.getByRole("heading", { name: tr("sv", "Your apps") })).toBeVisible();
    expect(await savedChoice(page)).toBeNull();

    // The person picks German: that is a saved choice, and it beats the instance default.
    await page.getByRole("button", { name: tr("sv", "Account") }).click();
    await page.getByRole("menuitem", { name: "Deutsch" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("heading", { name: tr("de", "Your apps") })).toBeVisible();
    expect(await savedChoice(page)).toBe("de");

    // Signed out, the sign-in page speaks the person's German although the server still says sv.
    await signOut(page);
    await expect(page.locator(META)).toHaveAttribute("content", "sv");
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(
      page.getByRole("heading", { name: tr("de", "Sign in"), exact: true }),
    ).toBeVisible();
    await expect(page.getByRole("heading", { name: tr("sv", "Sign in"), exact: true })).toHaveCount(
      0,
    );

    // And signing in again keeps it.
    await signInForm(page, "de", "sam");
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("heading", { name: tr("de", "Your apps") })).toBeVisible();
  });

  test("gives way to the language an administrator sets, which is not public", async ({
    page,
    request,
  }) => {
    const api = new Api(await playwrightRequest.newContext({ baseURL: languageUrl() }));
    const before = (await api.run("root", "setting.get", { id: "instance" })) as { value: string };
    try {
      const settings = { ...JSON.parse(before.value || "{}"), language: "fi" };
      await api.run("root", "setting.update", { id: "instance", value: JSON.stringify(settings) });

      // Before sign-in only the default of the server is known: the setting is not public.
      await page.goto(`${languageUrl()}/`);
      await expect(page.locator("html")).toHaveAttribute("lang", "sv");
      const unsigned = await request.post(`${languageUrl()}/api/setting/get`, {
        data: { id: "instance" },
      });
      expect(unsigned.status()).toBe(401);
      expect(await (await request.get(`${languageUrl()}/`)).text()).not.toContain('"fi"');

      // After sign-in the setting wins over the default of the server for a person with no choice.
      await signInForm(page, "sv", "sam");
      await expect(page.locator("html")).toHaveAttribute("lang", "fi");
      await expect(page.getByRole("heading", { name: tr("fi", "Your apps") })).toBeVisible();
      expect(await savedChoice(page)).toBeNull();

      // Signed out again, the screen before sign-in is back to the default of the server.
      await signOut(page);
      await expect(page.locator("html")).toHaveAttribute("lang", "sv");
    } finally {
      await api.run("root", "setting.update", { id: "instance", value: before.value });
    }
  });
});

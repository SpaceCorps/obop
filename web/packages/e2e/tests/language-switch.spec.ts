import type { Page } from "@playwright/test";
import { tr } from "../support/catalogue.ts";
import { languageUrl } from "../support/env.ts";
import { Api, expect, test } from "../support/fixtures.ts";

// Switching the language in the open app, without a reload, must leave nothing in the old language:
// what a reload shows is what the switch shows. The app names and their order are built once from
// the module list and the settings (App.tsx `buildApps`), the smart buttons of a record and a
// server-sorted list are cached per query; each one used to keep the old language until a reload.
//
// Every spec starts with no saved choice (`language: null`): a browser that asks for English. The
// instance of the first describe has OBOP_DEFAULT_LANGUAGE=sv (support/serve.ts, "language"), the
// others use the main instance, whose own default is the browser's.
test.use({ language: null, locale: "en-US", extraHTTPHeaders: { "Accept-Language": "en" } });

/** What the catalogue says in `lang` for an English message (English is the message itself). */
const say = (lang: string, id: string) => (lang === "en" ? id : tr(lang, id));
/** The name of a built-in app in `lang` (the message context "app", gettext's "\u0004" separator). */
const appName = (lang: string, en: string) => (lang === "en" ? en : tr(lang, `app\u0004${en}`));
/** The label of the menu of an app: "{app} menu". */
const menuLabel = (lang: string, app: string) => say(lang, "{app} menu").replace("{app}", app);
const NATIVE: Record<string, string> = { en: "English", sv: "Svenska" };

type Tile = { app: string; name: string };

/** The home screen: its "All apps" grid, in the order shown (the app id comes from the link). */
async function tiles(page: Page): Promise<Tile[]> {
  const grid = page.locator(".home .app-grid:not(.small) a.app-tile");
  await expect(grid.first()).toBeVisible();
  return grid.evaluateAll((els) =>
    els.map((e) => ({
      app: (e.getAttribute("href") ?? "").split("/")[1] ?? "",
      name: e.querySelector("strong")?.textContent ?? "",
    })),
  );
}

/** Home through the logo, a client-side navigation: the page is not reloaded. */
async function goHome(page: Page) {
  await page.locator("a.brand").click();
  await expect(page.locator(".home")).toBeVisible();
}

/** Customize > Theme, where Language & region lives, through the theme button of the top bar. */
async function openLanguageSettings(page: Page, lang: string) {
  await page.getByRole("button", { name: say(lang, "Theme"), exact: true }).click();
  await expect(page.getByRole("region", { name: say(lang, "Language & region") })).toBeVisible();
}

async function pickInSettings(page: Page, from: string, to: string) {
  const section = page.getByRole("region", { name: say(from, "Language & region") });
  await section.getByLabel(say(from, "Language")).selectOption(to);
  await expect(page.locator("html")).toHaveAttribute("lang", to);
}

async function pickInAccountMenu(page: Page, from: string, to: string) {
  await page.getByRole("button", { name: say(from, "Account"), exact: true }).click();
  await page.getByRole("menuitem", { name: NATIVE[to] }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", to);
}

/** What the server sends as the description of a model in `lang` (English is the description itself). */
async function describe(api: Api, model: string, lang: string): Promise<string> {
  const r = await api.get("/api/meta", await api.token("root"));
  const modules: any[] = Array.isArray(r.json) ? r.json : r.json.modules;
  const m = modules.flatMap((x) => x.models).find((x) => x.name === model);
  const text = lang === "en" ? m.description : m.descriptions?.[lang];
  expect(text, `a ${lang} description of ${model}`).toBeTruthy();
  return text;
}

const savedChoice = (page: Page) => page.evaluate(() => localStorage.getItem("obop-lang"));

/**
 * Switch from `from` to `to` in Settings > Language & region and look at everything that is built
 * from the app list, in that order, without reloading; then reload and compare with the home screen.
 */
async function switchInSettings(page: Page, api: Api, from: string, to: string) {
  await goHome(page);
  const before = await tiles(page);
  const invoicingFrom = appName(from, "Invoicing");
  expect(before.map((t) => t.name)).toContain(invoicingFrom);

  await openLanguageSettings(page, from);
  await pickInSettings(page, from, to);

  // The top bar names the open app (Customize) in the new language at once.
  await expect(page.locator(".brand-app")).toHaveText(appName(to, "Customize"));
  // The person has a choice of their own now, and the button that undoes it is there at once.
  const undo = page.getByRole("button", { name: say(to, "Use the instance default") });
  await expect(undo).toBeVisible();
  expect(await savedChoice(page)).toBe(to);

  // Home: the names and the order of the tiles.
  await goHome(page);
  const live = await tiles(page);
  expect(
    live.map((t) => t.app),
    "the order of the apps depends on the language",
  ).not.toEqual(before.map((t) => t.app));
  const invoicingTo = appName(to, "Invoicing");
  expect(live.find((t) => t.app === "invoicing")?.name).toBe(invoicingTo);
  expect(live.map((t) => t.name)).not.toContain(invoicingFrom);

  // The launcher lists the same apps in the same order.
  await page.getByRole("button", { name: say(to, "Open the app launcher") }).click();
  const launcher = page.locator(".launcher");
  await expect(launcher).toBeVisible();
  await expect(launcher.locator(".app-grid:not(.small) a.app-tile")).toHaveText(
    live.map((t) => t.name),
  );
  await page.keyboard.press("Escape");
  await expect(launcher).toHaveCount(0);

  // The command palette: "<app> › <model>" has the app in the new language too.
  await page.getByRole("button", { name: say(to, "Search apps, records and actions") }).click();
  const palette = page.locator(".palette");
  await palette.locator("input").fill(invoicingTo);
  await expect(palette.getByRole("option").first()).toContainText(invoicingTo);
  await expect(palette).not.toContainText(invoicingFrom);
  await page.keyboard.press("Escape");
  await expect(palette).toHaveCount(0);

  // The sidebar of an app: its header, its label for screen readers and the top bar.
  await page.locator(".home .app-grid:not(.small) a.app-tile", { hasText: invoicingTo }).click();
  await expect(page.locator("nav.sidebar .side-head strong")).toHaveText(invoicingTo);
  await expect(page.locator("nav.sidebar")).toHaveAttribute(
    "aria-label",
    menuLabel(to, invoicingTo),
  );
  await expect(page.locator(".brand-app")).toHaveText(invoicingTo);
  // The tooltip of a record in the sidebar is the description in the new language.
  await expect(page.locator('nav.sidebar a[href="#/invoicing/invoice"]')).toHaveAttribute(
    "title",
    await describe(api, "invoice", to),
  );

  // What a reload shows is what the switch showed.
  await goHome(page);
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("lang", to);
  expect(await tiles(page)).toEqual(live);

  // "Use the instance default" goes back to the default: no choice of their own, the old language.
  await openLanguageSettings(page, to);
  await page.getByRole("button", { name: say(to, "Use the instance default") }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", from);
  await expect(page.locator(".brand-app")).toHaveText(appName(from, "Customize"));
  await expect(page.getByRole("button", { name: say(to, "Use the instance default") })).toHaveCount(
    0,
  );
  await expect(
    page.getByRole("button", { name: say(from, "Use the instance default") }),
  ).toHaveCount(0);
  expect(await savedChoice(page)).toBeNull();
  await goHome(page);
  expect(await tiles(page)).toEqual(before);
}

test.describe("switching the language in Settings, without a reload", () => {
  test.describe("an instance whose default language is Swedish", () => {
    test.use({ baseURL: languageUrl() });

    test("Swedish to English: the app names and their order, the palette, the launcher, the sidebar", async ({
      page,
      signIn,
      api,
    }) => {
      await signIn("root", "/customize/theme?tab=theme");
      await expect(page.locator("html")).toHaveAttribute("lang", "sv");
      expect(await savedChoice(page)).toBeNull();
      await switchInSettings(page, api, "sv", "en");
    });
  });

  test("English to Swedish: the same, the other way", async ({ page, signIn, api }) => {
    await signIn("root", "/customize/theme?tab=theme");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    expect(await savedChoice(page)).toBeNull();
    await switchInSettings(page, api, "en", "sv");
  });
});

test.describe("switching the language with the account menu, on a screen that stays open", () => {
  test("the Apps tab follows, and leaving a name field as it is writes no setting", async ({
    page,
    signIn,
    api,
  }) => {
    const before = (await api.run("root", "setting.get", { id: "instance" })) as { value: string };
    const writes: string[] = [];
    page.on("request", (r) => {
      if (r.method() === "POST" && r.url().endsWith("/api/setting/update")) writes.push(r.url());
    });
    try {
      await signIn("root", "/customize/apps?tab=apps");
      const nameField = (lang: string, app: string) =>
        page.getByLabel(say(lang, "Name of {name}").replace("{name}", app), { exact: true });
      await expect(nameField("en", "Invoicing")).toHaveValue("Invoicing");

      await pickInAccountMenu(page, "en", "sv");

      // Every name, and every label built from it, is Swedish: the field shows what the app is called.
      const field = nameField("sv", "Fakturering");
      await expect(field).toHaveValue("Fakturering");
      await expect(
        page.getByLabel(say("sv", "Move {name} up").replace("{name}", "Fakturering")),
      ).toBeVisible();
      await expect(
        page.getByLabel(say("sv", "Colour of {name}").replace("{name}", "Fakturering")),
      ).toBeVisible();

      // Clicking into the field and out of it again changes nothing. Were the field to keep the
      // English name it was made with, this would save "Invoicing" as the name for everyone.
      await field.focus();
      await field.blur();
      // The save would be sent from the blur handler; give it room to show up before looking.
      await page.waitForTimeout(500);
      expect(writes, "a setting was written by leaving the field").toEqual([]);
      const after = (await api.run("root", "setting.get", { id: "instance" })) as {
        value: string;
      };
      expect(after.value).toBe(before.value);

      // A real rename still saves, and the field keeps what was typed.
      await field.fill("Fakturor");
      await field.blur();
      await expect
        .poll(async () => {
          const s = (await api.run("root", "setting.get", { id: "instance" })) as {
            value: string;
          };
          return JSON.parse(s.value || "{}").apps?.names?.invoicing;
        })
        .toBe("Fakturor");
      await expect(nameField("sv", "Fakturor")).toHaveValue("Fakturor");
    } finally {
      await api.run("root", "setting.update", { id: "instance", value: before.value });
    }
  });

  test("an open record: its related-record buttons follow", async ({ page, signIn, api, uid }) => {
    const party = await api.create("root", "party", { name: uid("Related") });
    await signIn("root", `/base/party?id=${party.id}`);
    const smart = async () => {
      const buttons = page.locator(".smart .smartbtn");
      await expect(buttons.first()).toBeVisible();
      // The counts come from the server; wait until they are all in.
      await expect(buttons.filter({ hasText: "…" })).toHaveCount(0);
      return buttons.evaluateAll((els) =>
        els.map((e) => ({ text: e.textContent, title: e.getAttribute("title") })),
      );
    };
    const english = await smart();
    expect(english.length).toBeGreaterThan(1);

    await pickInAccountMenu(page, "en", "sv");
    // The panel is the same one: no navigation, no reload.
    await expect(page.locator("aside.record")).toBeVisible();
    await expect(page.getByRole("group", { name: say("sv", "Related records") })).toBeVisible();
    const swedish = await smart();
    expect(swedish, "the buttons are in Swedish").not.toEqual(english);

    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "sv");
    expect(swedish, "what a reload shows").toEqual(await smart());
  });

  test("a list sorted on the server is sorted in the new language", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Sort data",
      fields: [{ name: "title", required: true }],
    });
    for (const t of ["Zebra", "Ärger", "Apfel", "Öle", "Item 10", "Item 2"])
      await api.create("root", model, { title: t });
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await signIn("root", `/${model}/${model}?view=list&st=${sorted}`);
    const titles = page.locator("table.list tbody tr td:nth-child(2)");
    // English puts the umlauts with their letters.
    await expect(titles).toHaveText(["Apfel", "Ärger", "Item 2", "Item 10", "Öle", "Zebra"]);

    await pickInAccountMenu(page, "en", "sv");
    // Swedish puts Ä and Ö after Z. The sort goes to the server with the language, so the list asks again.
    await expect(titles).toHaveText(["Apfel", "Item 2", "Item 10", "Zebra", "Ärger", "Öle"]);

    await page.reload();
    await expect(titles).toHaveText(["Apfel", "Item 2", "Item 10", "Zebra", "Ärger", "Öle"]);
  });
});

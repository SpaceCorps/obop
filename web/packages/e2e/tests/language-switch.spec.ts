import type { Locator, Page } from "@playwright/test";
import { tr } from "../support/catalogue.ts";
import { coreVersion, languageUrl } from "../support/env.ts";
import { Api, expect, test } from "../support/fixtures.ts";
import { commandUrl, isCommand } from "../support/urls.ts";

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
/** The words of a message in `lang` that are not placeholders, to look for in text that fills them in. */
const wordsOf = (lang: string, id: string) =>
  say(lang, id)
    .split(/\{[^}]+\}/)
    .map((w) => w.trim())
    .filter(Boolean);
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

/** The modules `/api/meta` lists, as the app gets them. */
async function modules(api: Api): Promise<any[]> {
  const r = await api.get("/api/meta", await api.token("root"));
  return Array.isArray(r.json) ? r.json : r.json.modules;
}

/**
 * The name of a module's app in `lang`, as the app builds it: the module's own label in
 * `/api/meta` when it has one, else the catalogue's word for the module's English name.
 */
async function moduleApp(api: Api, id: string, lang: string): Promise<string> {
  const m = (await modules(api)).find((x) => x.id === id);
  expect(m, `module ${id}`).toBeTruthy();
  return m.labels?.[lang] ?? appName(lang, m.name);
}

/** What the server sends as the description of a model in `lang` (English is the description itself). */
async function describe(api: Api, model: string, lang: string): Promise<string> {
  const m = (await modules(api)).flatMap((x) => x.models).find((x) => x.name === model);
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
  const invoicingFrom = await moduleApp(api, "invoicing", from);
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
  const invoicingTo = await moduleApp(api, "invoicing", to);
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
  test("the Apps tab follows; leaving a name field as it is writes nothing, a rename saves and keeps the field", async ({
    page,
    signIn,
    api,
  }) => {
    const settings = async () =>
      (await api.run("root", "setting.get", { id: "instance" })) as { value: string };
    const before = await settings();
    const names = async () => JSON.parse((await settings()).value || "{}").apps?.names ?? {};
    // Every write of a setting, in the order the page sent them. A request goes out in the handler
    // of the event that causes it, so what the page did earlier is listed before what it did later.
    const writes: string[] = [];
    page.on("request", (r) => {
      if (r.method() === "POST" && isCommand(r.url(), "setting", "update"))
        writes.push(JSON.parse(r.postData() ?? "{}").value);
    });
    const en = await moduleApp(api, "invoicing", "en");
    const sv = await moduleApp(api, "invoicing", "sv");
    expect(sv, "the app has another name in Swedish").not.toBe(en);
    try {
      await signIn("root", "/customize/apps?tab=apps");
      const nameField = (lang: string, app: string) =>
        page.getByLabel(say(lang, "Name of {name}").replace("{name}", app), { exact: true });
      await expect(nameField("en", en)).toHaveValue(en);

      await pickInAccountMenu(page, "en", "sv");

      // Every name, and every label built from it, is Swedish: the field shows what the app is called.
      const field = nameField("sv", sv);
      await expect(field).toHaveValue(sv);
      // The element itself, to see further down that a rename does not replace it.
      const node = await field.elementHandle();
      await expect(
        page.getByLabel(say("sv", "Move {name} up").replace("{name}", sv)),
      ).toBeVisible();
      await expect(
        page.getByLabel(say("sv", "Colour of {name}").replace("{name}", sv)),
      ).toBeVisible();

      // Clicking into the field and out of it again changes nothing. Were the field to keep the
      // English name it was made with, this would save it as the name for everyone.
      await field.focus();
      await field.blur();

      // A real rename saves. It is the first write the page made, so the blur above wrote nothing:
      // a write it had sent would be listed before this one.
      const renamed = `${sv} 2`;
      await field.fill(renamed);
      await field.blur();
      await expect.poll(async () => (await names()).invoicing).toBe(renamed);
      expect(writes.map((w) => JSON.parse(w).apps?.names)).toEqual([{ invoicing: renamed }]);

      // The saved name is in the app list now (the label of the field follows it), and the field is
      // the element the person was typing in. Made again, it would take what is typed next with it.
      const saved = nameField("sv", renamed);
      await expect(saved).toHaveValue(renamed);
      expect(await node!.evaluate((n) => n.isConnected), "the field was made again").toBe(true);
      await saved.focus();
      await page.keyboard.press("End");
      await page.keyboard.type("!");
      await expect(saved).toHaveValue(`${renamed}!`);
      await saved.blur();
      await expect.poll(async () => (await names()).invoicing).toBe(`${renamed}!`);

      // Resetting the launcher puts the name of the language back, in the same field.
      await page.getByRole("button", { name: say("sv", "Reset the launcher") }).click();
      await expect(nameField("sv", sv)).toHaveValue(sv);
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

  test("a list with no sort is not asked for again: nothing in its answer depends on the language", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Plain data",
      fields: [{ name: "title", required: true }],
    });
    for (const t of ["Zebra", "Ärger", "Apfel"]) await api.create("root", model, { title: t });
    await signIn("root", `/${model}/${model}?view=list`);
    const titles = page.locator("table.list tbody tr td:nth-child(2)");
    await expect(titles).toHaveCount(3);

    const asked: { order?: unknown[]; lang?: string }[] = [];
    page.on("request", (r) => {
      if (r.method() === "POST" && isCommand(r.url(), model, "search"))
        asked.push(JSON.parse(r.postData() ?? "{}"));
    });
    await pickInAccountMenu(page, "en", "sv");

    // Sort it now (a client-side navigation). The requests are listed in the order they were sent,
    // so the one that sorts is the first after the switch when the switch itself asked for nothing.
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await page.evaluate((h) => (location.hash = h), `/${model}/${model}?view=list&st=${sorted}`);
    await expect(titles).toHaveText(["Apfel", "Zebra", "Ärger"]);
    expect(asked).toHaveLength(1);
    expect(asked[0]!.order).toHaveLength(1);
    expect(asked[0]!.lang).toMatch(/^sv/);
  });

  test("a sorted board orders the cards of a column in the new language", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Board data",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "won"] },
      ],
    });
    for (const t of ["Zebra", "Ärger", "Apfel", "Öle"])
      await api.create("root", model, { title: t, state: "new" });
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await signIn("root", `/${model}/${model}?view=kanban&st=${sorted}`);
    // Every card is in the one column that has any (the names of the columns are translated).
    const cards = page.locator(".kcard .ktitle");
    await expect(cards).toHaveText(["Apfel", "Ärger", "Öle", "Zebra"]);

    await pickInAccountMenu(page, "en", "sv");
    await expect(cards).toHaveText(["Apfel", "Zebra", "Ärger", "Öle"]);
  });

  test("a board with no sort is not asked for again: its columns and cards do not depend on the language", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Plain board",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "won"] },
      ],
    });
    for (const t of ["Zebra", "Ärger", "Apfel", "Öle"])
      await api.create("root", model, { title: t, state: "new" });
    await signIn("root", `/${model}/${model}?view=kanban`);
    const cards = page.locator(".kcard .ktitle");
    await expect(cards).toHaveCount(4);

    // Every request for the model's data after this point, in the order it was sent.
    const asked: { command: string; order?: unknown[]; lang?: string }[] = [];
    page.on("request", (r) => {
      if (r.method() !== "POST") return;
      for (const command of ["search", "aggregate"])
        if (isCommand(r.url(), model, command))
          asked.push({ command, ...JSON.parse(r.postData() ?? "{}") });
    });
    await pickInAccountMenu(page, "en", "sv");

    // Sort the board now (a client-side navigation). The first request after the switch is the one
    // that sorts, so the switch itself asked for nothing: neither the columns nor their cards.
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await page.evaluate((h) => (location.hash = h), `/${model}/${model}?view=kanban&st=${sorted}`);
    await expect(cards).toHaveText(["Apfel", "Zebra", "Ärger", "Öle"]);
    expect(asked.map((a) => a.command)).not.toContain("aggregate");
    expect(asked.length).toBeGreaterThan(0);
    for (const a of asked) {
      expect(a.command).toBe("search");
      expect(a.order, "a request that does not sort").toHaveLength(1);
      expect(a.lang).toMatch(/^sv/);
    }
  });
});

test.describe("a record panel that stays open while the language changes", () => {
  // The name of the panel is translated ("{model} record"): find it by what it is.
  const panelOf = (page: Page) => page.locator("aside.record");

  test("a field that failed its check is told again in the new language", async ({
    page,
    signIn,
    scratchModule,
  }) => {
    // A field with no translation anywhere, so its title is the same in both languages.
    const model = await scratchModule({
      title: "Check data",
      fields: [{ name: "zork", required: true }],
    });
    await signIn("root", `/${model}/${model}?view=list&id=new`);
    const panel = panelOf(page);
    await panel.getByRole("button", { name: say("en", "Create") }).click();
    const alert = panel.locator(".field p.error");
    await expect(alert).toHaveText(say("en", "{field} is required").replace("{field}", "Zork"));

    await pickInAccountMenu(page, "en", "sv");
    await expect(panelOf(page)).toBeVisible();
    // The same message, worded by the Swedish catalogue: its words around the name of the field.
    await expect(alert).toContainText("Zork");
    for (const w of wordsOf("sv", "{field} is required")) await expect(alert).toContainText(w);
    await expect(alert).not.toContainText(wordsOf("en", "{field} is required")[0]!);
  });

  test("the server's refusal in the banner is worded again in the new language", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule({
      title: "Refusal data",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "doing", "done"] },
      ],
      actions: [{ name: "start", field: "state", from: "new", to: "doing" }],
    });
    const rec = await api.create("root", model, { title: uid("Stale"), state: "new" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    // Swedish first, and back to English after: a sentence kept in the language it was worded in
    // is Swedish by then, which nothing words in English again (a kept English one would be worded
    // in Swedish, because the server's English is what the rules match).
    await pickInAccountMenu(page, "en", "sv");
    const steps = panelOf(page).locator("ol.statusbar button");
    await expect(steps.nth(1)).not.toHaveAttribute("aria-disabled", "true");

    // Someone else moves it on while this page is open; the move the page offers is refused.
    await api.run("root", `${model}.start`, { id: rec.id });
    await steps.nth(1).click();
    const banner = panelOf(page).locator("p.error.banner");
    await expect(banner).toBeVisible();
    const refusal = "{field} must be {from} to do this, it is {current}";
    for (const w of wordsOf("sv", refusal)) await expect(banner).toContainText(w);
    const swedish = (await banner.textContent())!;

    await pickInAccountMenu(page, "sv", "en");
    await expect(panelOf(page)).toBeVisible();
    for (const w of wordsOf("en", refusal)) await expect(banner).toContainText(w);
    await expect(banner).not.toHaveText(swedish);

    // And forward again: the sentence is made from the server's words each time.
    await pickInAccountMenu(page, "en", "sv");
    await expect(banner).toHaveText(swedish);
  });

  test("an open record stays open, with what was typed in it, when the sorted list around it asks again", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Open data",
      fields: [{ name: "title", required: true }, { name: "note" }],
    });
    await api.create("root", model, { title: "Ärger" });
    const others = [
      "Apfel",
      ...Array.from({ length: 28 }, (_, i) => `Item ${String(i + 1).padStart(2, "0")}`),
    ];
    for (const t of others) await api.create("root", model, { title: t });
    // Pages of 25: the record is on the first page in English (A, Ä, Item...) and on the second in
    // Swedish (Ä sorts after Z), so the new first page does not hold it.
    await page.addInitScript(() => localStorage.setItem("obop-pagesize", "25"));
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await signIn("root", `/${model}/${model}?view=list&st=${sorted}`);
    const titles = page.locator("table.list tbody tr td:nth-child(2)");
    await expect(titles).toHaveCount(25);
    await expect(titles.nth(1)).toHaveText("Ärger");
    // Opened from the list, as a person does: the record is known from the list's page only, and
    // was never fetched on its own, so nothing else keeps it when the page no longer has it.
    await titles.nth(1).click();
    await expect(panelOf(page).locator("h2")).toHaveText("Ärger");
    const note = page.locator(`#f-${model}-note`);
    await note.fill("not saved yet");

    await pickInAccountMenu(page, "en", "sv");
    // The list has asked again and the record is no longer on its page...
    await expect(titles.filter({ hasText: "Ärger" })).toHaveCount(0);
    // ...and the panel is the same one, with its draft.
    await expect(panelOf(page).locator("h2")).toHaveText("Ärger");
    await expect(note).toHaveValue("not saved yet");
  });

  /**
   * The banner that is on screen, in Swedish, goes to English and back: the sentence is made from
   * the server's words each time. A sentence kept in the language it was worded in is Swedish by the
   * time English is asked for, which nothing words in English again.
   */
  async function swedishEnglishSwedish(page: Page, banner: Locator, refusal: string) {
    await expect(banner).toBeVisible();
    for (const w of wordsOf("sv", refusal)) await expect(banner).toContainText(w);
    const swedish = (await banner.textContent())!;
    await pickInAccountMenu(page, "sv", "en");
    await expect(panelOf(page)).toBeVisible();
    for (const w of wordsOf("en", refusal)) await expect(banner).toContainText(w);
    await expect(banner).not.toHaveText(swedish);
    await pickInAccountMenu(page, "en", "sv");
    await expect(banner).toHaveText(swedish);
  }

  const REFUSAL = "{field} must be {from} to do this, it is {current}";

  test("the refusal of a move made with the Move to menu is worded again in the new language", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    // A governed field that is not the status: the panel shows it with a menu (GovernedValue).
    const model = await scratchModule({
      title: "Menu data",
      fields: [
        { name: "title", required: true },
        { name: "phase", kind: "select", options: ["draft", "review", "final"] },
      ],
      actions: [
        { name: "submit", field: "phase", from: "draft", to: "review" },
        { name: "sign_off", field: "phase", from: "review", to: "final", approval: true },
      ],
    });
    const rec = await api.create("root", model, { title: uid("Menu"), phase: "draft" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    await pickInAccountMenu(page, "en", "sv");
    await expect(panelOf(page).locator(".governed button")).toBeVisible();

    // Someone else moves it on while this page is open; the move the menu offers is refused.
    await api.run("root", `${model}.submit`, { id: rec.id });
    await panelOf(page).locator(".governed button").click();
    await page.getByRole("menuitem").first().click();
    await swedishEnglishSwedish(page, panelOf(page).locator("p.error.banner"), REFUSAL);
  });

  test("the refusal of an action button is worded again in the new language", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule({
      title: "Button data",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "doing", "done"] },
      ],
      actions: [{ name: "start", field: "state", from: "new", to: "doing" }],
    });
    const rec = await api.create("root", model, { title: uid("Button"), state: "new" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    await pickInAccountMenu(page, "en", "sv");
    const start = panelOf(page).locator(".actionrow button").first();
    await expect(start).not.toHaveAttribute("aria-disabled", "true");

    await api.run("root", `${model}.start`, { id: rec.id });
    await start.click();
    await swedishEnglishSwedish(page, panelOf(page).locator("p.error.banner"), REFUSAL);
  });

  test("the server's refusal of a field is worded again in the new language", async ({
    page,
    signIn,
    api,
    scratchPackage,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const q = JSON.stringify;
    const id = await scratchPackage(
      (pkg) => `
[module]
id = ${q(pkg)}
version = "0.1.0"
name = "Field refusal"
description = "A field with a limit (end-to-end test module)"
author = "obop e2e"
requires_core = ${q(`^${coreVersion}`)}

[[model]]
name = "${pkg}_note"
description = "A note"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "headline"
  description = "A headline"
  class = "none"

  [[model.field]]
  name = "code"
  description = "A short code"
  class = "none"
  max_length = 10
`,
    );
    const model = `${id}_note`;
    // A page loaded before the limit was set: its copy of /api/meta has none on the code, so the
    // browser lets the text go and the server refuses it, naming the field.
    await page.route(/\/api\/meta(\?.*)?$/, async (route) => {
      const res = await route.fetch();
      const body = await res.json();
      for (const m of body.modules)
        for (const d of m.models)
          for (const f of d.fields) if (f.name === "code") delete f.max_length;
      await route.fulfill({ response: res, json: body });
    });
    await signIn("root", `/${id}/${model}?view=list&id=new`);
    await pickInAccountMenu(page, "en", "sv");
    await page.locator(`#f-${model}-headline`).fill(uid("Refused"));
    await page.locator(`#f-${model}-code`).fill("abcdefghijk");
    await panelOf(page)
      .getByRole("button", { name: say("sv", "Create") })
      .click();
    const alert = page.locator(`#f-${model}-code-err`);
    await expect(alert).toBeVisible();

    // The server's words are English: shown as they came in English, worded by the catalogue in Swedish.
    const english = `${model}.code must be at most 10 characters; this has 11`;
    await expect(alert).not.toContainText("must be at most");
    await expect(alert).toContainText("10");
    const swedish = (await alert.textContent())!;
    await pickInAccountMenu(page, "sv", "en");
    await expect(alert).toHaveText(english);
    await pickInAccountMenu(page, "en", "sv");
    await expect(alert).toHaveText(swedish);
    expect((await api.list("root", model)).length).toBe(0);
  });

  test("a record that is not on the page yet does not show the row of the one before it", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Next data",
      fields: [{ name: "title", required: true }],
    });
    const rows = [];
    for (let i = 1; i <= 27; i++)
      rows.push(await api.create("root", model, { title: `Item ${String(i).padStart(2, "0")}` }));
    // Pages of 25: "Item 01" is on the first page, "Item 27" on the second.
    await page.addInitScript(() => localStorage.setItem("obop-pagesize", "25"));
    const sorted = encodeURIComponent(JSON.stringify({ s: { f: "title", dir: "asc" } }));
    await signIn("root", `/${model}/${model}?view=list&st=${sorted}`);
    const titles = page.locator("table.list tbody tr td:nth-child(2)");
    await expect(titles).toHaveCount(25);
    // Opened from the list: the page remembers the row of "Item 01".
    await titles.first().click();
    await expect(panelOf(page).locator("h2")).toHaveText("Item 01");

    // Another record of the same model, one the page has not got: its `get` is held back.
    let release!: () => void;
    const held = new Promise<void>((resolve) => (release = resolve));
    await page.route(commandUrl(model, "get"), async (route) => {
      await held;
      await route.continue();
    });
    await page.evaluate(
      (h) => (location.hash = h),
      `/${model}/${model}?view=list&st=${sorted}&id=${rows[26]!.id}`,
    );
    // While it is held there is nothing to show for it: not the row of "Item 01" under its address.
    await expect(panelOf(page)).toHaveCount(0);
    release();
    await expect(panelOf(page).locator("h2")).toHaveText("Item 27");
  });
});

import type { Page } from "@playwright/test";
import { tr } from "../support/catalogue.ts";
import { platformUrl } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

/** Open a screen of the signed-in app in a language (`?lang=` overrides the saved choice). */
async function visit(page: Page, lang: string, hash = "/") {
  await page.goto(`/?lang=${lang}#${hash}`);
  await expect(page.locator(".topbar")).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", lang);
}

test.describe("language choice", () => {
  test("the account menu switches the language and the choice survives a reload", async ({
    page,
    signIn,
    context,
  }) => {
    await signIn("sam");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();

    await page.getByRole("button", { name: "Account" }).click();
    await page.getByRole("menuitem", { name: "Deutsch" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
    await expect(page.getByRole("heading", { name: tr("de", "Your apps") })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Your apps" })).toHaveCount(0);

    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("heading", { name: tr("de", "Your apps") })).toBeVisible();

    // The server can read the choice from a cookie, and a person's own choice is stored in the browser.
    const cookie = (await context.cookies()).find((c) => c.name === "obop_lang");
    expect(cookie?.value).toBe("de");
    expect(cookie?.sameSite).toBe("Lax");
    expect(await page.evaluate(() => localStorage.getItem("obop-lang"))).toBe("de");
  });

  test("?lang= overrides the saved choice for a shared link, and an unknown language is ignored", async ({
    page,
    signIn,
  }) => {
    await signIn("sam");
    await visit(page, "fi");
    await expect(page.getByRole("heading", { name: tr("fi", "Your apps") })).toBeVisible();
    await page.goto("/?lang=xx#/");
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
  });

  test.describe("first visit in a Polish browser", () => {
    test.use({ locale: "pl-PL", language: null });
    test("the language is detected from the browser", async ({ page, signIn }) => {
      await signIn("sam");
      await expect(page.locator("html")).toHaveAttribute("lang", "pl");
      await expect(page.getByRole("heading", { name: tr("pl", "Your apps") })).toBeVisible();
    });
  });

  test("the sign-in page has a language switch", async ({ page }) => {
    await page.goto("/");
    await page.getByLabel("Language").selectOption("sv");
    await expect(page.locator("html")).toHaveAttribute("lang", "sv");
    await expect(page.getByRole("heading", { name: tr("sv", "Sign in") })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("heading", { name: tr("sv", "Sign in") })).toBeVisible();
  });

  test("Customize > Theme sets language, region and, for an admin, the instance default", async ({
    page,
    signIn,
    api,
  }) => {
    await signIn("root", "/customize/theme?tab=theme");
    const section = page.getByRole("region", { name: "Language & region" });
    await expect(section).toBeVisible();
    await section.getByLabel("Language").selectOption("fr");
    await expect(page.locator("html")).toHaveAttribute("lang", "fr");

    // Region is independent of language: a French interface with Swiss formats.
    await page.getByLabel(tr("fr", "Region and formats", "ui")).selectOption("de-CH");
    await expect(page.getByText(/^(Exemple|Example)/)).toContainText(/1['’]234['’]567\.89/);

    // The admin can make a language the default for everyone who has not chosen one.
    const before = (await api.run("root", "setting.get", { id: "instance" })) as { value: string };
    try {
      await page.getByRole("button", { name: tr("fr", "Set instance default") }).click();
      await expect
        .poll(async () => {
          const s = (await api.run("root", "setting.get", { id: "instance" })) as { value: string };
          return JSON.parse(s.value).language;
        })
        .toBe("fr");
    } finally {
      await api.run("root", "setting.update", { id: "instance", value: before.value });
    }
  });
});

test.describe("translated screens", () => {
  const widths = [
    { name: "1440", width: 1440, height: 900 },
    { name: "768", width: 768, height: 1024 },
    { name: "390", width: 390, height: 844 },
  ];
  for (const lang of ["de", "fi"]) {
    for (const vp of widths) {
      test(`${lang} at ${vp.name}: translated chrome, no sideways scroll, no cut-off controls`, async ({
        page,
        signIn,
        api,
        uid,
      }) => {
        await api.create("root", "lead", {
          title: uid("Rahmenvertrag"),
          expected_revenue_minor: 123456,
        });
        await page.setViewportSize({ width: vp.width, height: vp.height });
        await signIn("root");
        const t = (id: string) => tr(lang, id);
        const check = async (screen: string) =>
          expect(await layoutProblems(page), `${lang} ${vp.name} ${screen}`).toEqual([]);

        await visit(page, lang, "/");
        await expect(page.getByRole("heading", { name: t("Your apps") })).toBeVisible();
        await check("home");

        await visit(page, lang, "/crm/lead?view=list");
        await expect(page.getByRole("button", { name: t("New") }).first()).toBeVisible();
        await expect(page.getByRole("grid", { name: /./ })).toBeVisible();
        await check("list");

        await visit(page, lang, "/crm/lead?view=kanban");
        await expect(page.locator(".kanban")).toBeVisible();
        await check("kanban");

        await visit(page, lang, "/crm/lead?view=list&id=new");
        await expect(page.locator(".record")).toBeVisible();
        await expect(
          page.locator(".record").getByRole("button", { name: t("Create") }),
        ).toBeVisible();
        await check("record panel");

        await visit(page, lang, "/crm/lead?view=list");
        await page.getByRole("button", { name: t("Filters, group by and favorites") }).click();
        await page.getByRole("button", { name: t("Add custom filter") }).click();
        await expect(page.getByRole("heading", { name: t("Group by") })).toBeVisible();
        await check("filter panel");

        await visit(page, lang, "/customize/theme?tab=theme");
        await expect(page.getByRole("heading", { name: t("Language & region") })).toBeVisible();
        await check("customize");
      });
    }
  }

  test("the sign-up app switches language and keeps it", async ({ page }) => {
    await page.goto(`${platformUrl()}/app/`);
    await page.getByRole("button", { name: "Language" }).click();
    await page.getByRole("menuitem", { name: "Suomi" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "fi");
    await expect(
      page.getByRole("button", { name: tr("fi", "Create account", "platform") }),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByRole("button", { name: tr("fi", "Create account", "platform") }),
    ).toBeVisible();
  });
});

test.describe("numbers, money and dates", () => {
  test("a German money field accepts 12,50 and stores 1250 minor units", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const title = uid("Preis");
    await signIn("sam");
    await visit(page, "de", "/crm/lead?view=list&id=new");
    const panel = page.locator(".record");
    await panel.getByLabel(tr("de", "field\u0004Title")).fill(title);
    const amount = panel.getByLabel(tr("de", "field\u0004Expected revenue"));
    await amount.fill("12,50");
    await expect(panel.locator(".money-read")).toHaveCount(0);
    await panel.getByRole("button", { name: tr("de", "Create") }).click();
    await expect(page.locator(".toast")).toBeVisible();

    const leads = await api.list("sam", "lead");
    expect(leads.find((l) => l.title === title)?.expected_revenue_minor).toBe(1250);
    // It reads back the way a German reads it, with a no-break space before the euro sign.
    await expect(page.locator("tr", { hasText: title })).toContainText(/12,50\s€/);

    // A group mark that is not the locale's own is shown as read before anything is saved.
    await visit(page, "de", "/crm/lead?view=list&id=new");
    await panel.getByLabel(tr("de", "field\u0004Title")).fill(uid("Mehrdeutig"));
    await panel.getByLabel(tr("de", "field\u0004Expected revenue")).fill("1,250");
    await expect(panel.locator(".money-read")).toContainText(/1\.250,00\s€/);
    await panel.getByLabel(tr("de", "field\u0004Expected revenue")).fill("abc");
    await panel.getByRole("button", { name: tr("de", "Create") }).click();
    await expect(panel.getByRole("alert")).toContainText("12,50");
  });

  test("dates, numbers and money follow the language and, separately, the region", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Format data",
      fields: [
        { name: "title", required: true },
        { name: "due", kind: "date" },
        { name: "amount_minor", kind: "money" },
        { name: "qty", kind: "number" },
      ],
    });
    const title = uid("Row");
    await api.create("root", model, {
      title,
      due: "2026-03-05",
      amount_minor: 123456789,
      qty: 1234567,
    });
    await signIn("root");

    const expected = (loc: string) => ({
      date: new Intl.DateTimeFormat(loc, {
        year: "numeric",
        month: "short",
        day: "numeric",
      }).format(new Date(2026, 2, 5)),
      money: new Intl.NumberFormat(loc, { style: "currency", currency: "EUR" }).format(1234567.89),
      qty: new Intl.NumberFormat(loc, { maximumFractionDigits: 0 }).format(1234567),
    });
    for (const [lang, loc] of [
      ["de", "de-DE"],
      ["fi", "fi-FI"],
      ["fr", "fr-FR"],
      ["pl", "pl-PL"],
    ] as const) {
      await visit(page, lang, `/${model}/${model}?view=list`);
      const row = page.locator("tr", { hasText: title });
      const e = expected(loc);
      await expect(row, `${lang} date`).toContainText(e.date);
      await expect(row, `${lang} money`).toContainText(e.money);
      await expect(row, `${lang} number`).toContainText(e.qty);
    }

    // German interface, Swiss formats.
    await page.goto(`/?lang=de#/${model}/${model}?view=list`);
    await page.evaluate(() => localStorage.setItem("obop-region", "de-CH"));
    await page.reload();
    const ch = expected("de-CH");
    await expect(page.locator("tr", { hasText: title })).toContainText(ch.money);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
  });

  // A search over every field runs in the browser, which sorts with the language's collation
  // (the server pages and sorts the plain list in database order, D49).
  test("a list sorted in the browser uses the collation of the language", async ({
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
    await signIn("root");
    await visit(
      page,
      "de",
      `/${model}/${model}?view=list&st=${encodeURIComponent(JSON.stringify({ q: [{ op: "search", v: "e" }], s: { f: "title", dir: "asc" } }))}`,
    );
    const names = page.locator("table.list tbody tr td:nth-child(2)");
    await expect(names).toHaveText(["Apfel", "Ärger", "Item 2", "Item 10", "Öle", "Zebra"]);
  });
});

test.describe("plurals", () => {
  test("Polish counts read 1 rekord, 2 rekordy, 5 rekordów", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Plural data",
      fields: [{ name: "title", required: true }],
    });
    for (const [word, n] of [
      ["one", 1],
      ["two", 2],
      ["five", 5],
    ] as const)
      for (let i = 0; i < n; i++) await api.create("root", model, { title: `${word}-x ${i}` });
    await signIn("root");
    await visit(page, "pl", `/${model}/${model}?view=list`);
    const count = page.locator(".controlpanel .count");
    await expect(count).toHaveText("8 rekordów");
    const search = page.getByRole("combobox", { name: /./ }).first();
    for (const [word, text] of [
      ["one-x", "1 rekord"],
      ["two-x", "2 rekordy"],
      ["five-x", "5 rekordów"],
    ] as const) {
      await search.fill(word);
      await search.press("Enter");
      await expect(count).toHaveText(text);
      await page.locator(".facet button").first().click();
    }
  });
});

test.describe("pseudo-locale", () => {
  /** Visible text that is neither accented pseudo text nor data (records, names, numbers). */
  async function plainText(page: Page): Promise<string[]> {
    return page.evaluate(() => {
      const accent = /[áƀçðéƒĝĥíĵķļɱñöþǫŕšţúṽŵẋýžßƑṀÁÇÐÉÍÑÖÞÚÝŠŽ]/;
      const out: string[] = [];
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const text = (n.textContent ?? "").trim();
        if (!/\p{L}{2}/u.test(text) || accent.test(text)) continue;
        const el = n.parentElement;
        if (
          !el ||
          el.closest(
            "script, style, [lang=en], td, th[scope=row], code, kbd, .avatar, .ktitle, .kval, .masked, .record header h2, .crumbs strong, .brand, .sr-only, option, .gname, .badge",
          )
        )
          continue;
        const r = el.getBoundingClientRect();
        if (!r.width || !r.height || getComputedStyle(el).visibility === "hidden") continue;
        out.push(text);
      }
      return out;
    });
  }

  test("every interface string on the key screens is accented pseudo text", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await api.create("root", "lead", { title: uid("Pseudo"), expected_revenue_minor: 5000 });
    await signIn("root");
    const screens: [string, () => Promise<void>][] = [
      ["home", async () => visit(page, "en-XA", "/")],
      ["list", async () => visit(page, "en-XA", "/crm/lead?view=list")],
      ["kanban", async () => visit(page, "en-XA", "/crm/lead?view=kanban")],
      ["record", async () => visit(page, "en-XA", "/crm/lead?view=list&id=new")],
      [
        "filters",
        async () => {
          await visit(page, "en-XA", "/crm/lead?view=list");
          await page.locator(".caretbtn").click();
          await page.locator(".fp-custom button").click();
        },
      ],
      ["customize", async () => visit(page, "en-XA", "/customize/theme?tab=theme")],
      [
        "menu",
        async () => {
          await visit(page, "en-XA", "/crm/lead?view=list");
          await page.locator(".avatar-btn").click();
        },
      ],
    ];
    for (const [name, go] of screens) {
      await go();
      await expect(page.locator("html")).toHaveAttribute("lang", "en-XA");
      await page.waitForTimeout(150);
      // Language names are written in their own language on purpose, and "root" is the user.
      const names = new Set([
        "root",
        "English",
        "Deutsch",
        "Français",
        "Polski",
        "Suomi",
        "Svenska",
        "Español",
      ]);
      const plain = (await plainText(page)).filter((s) => !names.has(s));
      expect(plain, `untranslated text on ${name}`).toEqual([]);
    }
  });

  test("the mirrored pseudo-locale sets dir=rtl and the layout follows", async ({
    page,
    signIn,
  }) => {
    await signIn("root");
    await page.goto("/?lang=ar-XB#/crm/lead?view=list");
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await expect(page.locator("html")).toHaveAttribute("lang", "ar-XB");
    // The side panel sits at the start edge in a right-to-left page: the sidebar is on the right.
    const side = await page.locator(".sidebar").boundingBox();
    const main = await page.locator(".main").boundingBox();
    expect(side!.x).toBeGreaterThan(main!.x);
    expect(await layoutProblems(page)).toEqual([]);
  });
});

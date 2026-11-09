// The public website (web/apps/site, served at the root of the control plane): static HTML that
// must read and navigate without JavaScript, be well-formed for crawlers and agents, and be
// localised. Runs against the built web/site directory.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { messages } from "../../../apps/site/i18n/index.ts";
import { platformUrl, repoRoot, siteDir } from "../support/env.ts";
import { expect, test, unnamedControls } from "../support/fixtures.ts";

const base = () => platformUrl();

/** The product's own name for the Field Service module in German (packages/labels/de/field_service.toml). */
const DE_FIELD_SERVICE = (() => {
  const toml = readFileSync(join(repoRoot, "packages/labels/de/field_service.toml"), "utf8");
  return /\[labels\.de\.module\]\s*\nname\s*=\s*"([^"]+)"/.exec(toml)![1]!;
})();

/** URLs of every page of one language, from its sitemap. */
async function pagesOf(request: import("@playwright/test").APIRequestContext, locale: string) {
  const xml = await (await request.get(`${base()}/sitemap-${locale}.xml`)).text();
  return [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => new URL(m[1]!).pathname);
}

/**
 * The contrast ratio of what is drawn. Colours are read back through a canvas, so tokens written as
 * oklch() or color-mix() count too. The text is mixed with its background (every translucent layer
 * up to the first opaque one) by the opacity of the element and of every ancestor, and a filter (the
 * primary pill's hover is brightness(1.08)) scales the colours it covers, so what is measured is what
 * is drawn and not only what the style sheet says. With a selector: that element. Without one: every
 * text node of <main>, with every fold opened, so a style nobody listed cannot slip through.
 */
async function contrast(page: import("@playwright/test").Page, selector: string | null) {
  return page.evaluate((sel) => {
    const c = document.createElement("canvas");
    c.width = c.height = 1;
    const ctx = c.getContext("2d", { willReadFrequently: true })!;
    const rgba = (color: string) => {
      ctx.clearRect(0, 0, 1, 1);
      ctx.fillStyle = "#000";
      ctx.fillStyle = color;
      ctx.fillRect(0, 0, 1, 1);
      return [...ctx.getImageData(0, 0, 1, 1).data];
    };
    const lum = (d: number[]) => {
      const f = (v: number) => {
        const x = Math.min(255, v) / 255;
        return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(d[0]!) + 0.7152 * f(d[1]!) + 0.0722 * f(d[2]!);
    };
    const brightness = (n: Element | null) => {
      let f = 1;
      for (; n; n = n.parentElement) {
        const m = /brightness\(([\d.]+)\)/.exec(getComputedStyle(n).filter);
        if (m) f *= Number(m[1]);
      }
      return f;
    };
    const ratio = (el: Element) => {
      // The background: translucent layers over the first opaque one (or the white of the page).
      const layers: number[][] = [];
      let bgNode: Element | null = null;
      for (let n: Element | null = el; n; n = n.parentElement) {
        const d = rgba(getComputedStyle(n).backgroundColor);
        if (d[3]! > 0) layers.push(d);
        if (d[3] === 255) {
          bgNode = n;
          break;
        }
      }
      let bg = [255, 255, 255];
      for (const d of layers.reverse()) {
        const a = d[3]! / 255;
        bg = [0, 1, 2].map((i) => a * d[i]! + (1 - a) * bg[i]!);
      }
      let opacity = 1;
      for (let n: Element | null = el; n; n = n.parentElement)
        opacity *= parseFloat(getComputedStyle(n).opacity);
      const fg = rgba(getComputedStyle(el).color);
      const a = (fg[3]! / 255) * opacity;
      const mixed = [0, 1, 2].map((i) => a * fg[i]! + (1 - a) * bg[i]!);
      const fgScale = brightness(el);
      const bgScale = brightness(bgNode);
      const [l1, l2] = [lum(mixed.map((v) => v * fgScale)), lum(bg.map((v) => v * bgScale))];
      return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    };
    if (sel) {
      const el = document.querySelector(sel);
      return el ? { texts: 1, worst: [{ at: sel, ratio: ratio(el) }] } : null;
    }
    document
      .querySelectorAll("main details")
      .forEach((d) => ((d as HTMLDetailsElement).open = true));
    const walker = document.createTreeWalker(document.querySelector("main")!, NodeFilter.SHOW_TEXT);
    const seen = new Map<Element, string>();
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const el = n.parentElement;
      // Text with no letter or digit (the zero-width space of a heading's permalink) draws nothing.
      if (!el || !/[\p{L}\p{N}]/u.test(n.textContent ?? "") || el.closest("svg, script, style"))
        continue;
      const r = el.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) continue; // text for screen readers only
      if (!el.checkVisibility({ visibilityProperty: true })) continue;
      if (!seen.has(el)) seen.set(el, (n.textContent ?? "").trim().slice(0, 24));
    }
    const all = [...seen].map(([el, text]) => ({
      at: `${el.tagName.toLowerCase()}${typeof el.className === "string" && el.className ? "." + el.className.split(" ")[0] : ""} "${text}"`,
      ratio: ratio(el),
    }));
    all.sort((a, b) => a.ratio - b.ratio);
    return { texts: all.length, worst: all.slice(0, 5) };
  }, selector);
}

test.describe("website without JavaScript", () => {
  test.use({ javaScriptEnabled: false });

  test("the home page is real HTML: headline, 28 apps, FAQ, pricing, no framework script", async ({
    page,
    request,
  }) => {
    const html = await (await request.get(`${base()}/`)).text();
    expect(html).toContain("One business.");
    expect(html).not.toMatch(/<script[^>]+type="module"[^>]+\/assets\//);

    await page.goto(`${base()}/`);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("One platform.");
    await expect(page.locator("#apps .app")).toHaveCount(28);
    await expect(page.locator("#apps .app.live")).toHaveCount(6);
    await expect(page.locator("#apps .app.market")).toHaveCount(22);
    await expect(
      page.getByText(
        "Planned prices per server and month, excluding VAT. Hosting is not offered or charged yet.",
      ),
    ).toBeVisible();
    await expect(page.locator("#faq details")).toHaveCount(8);
    await page.locator("#faq summary").first().click();
    await expect(page.locator("#faq details").first()).toHaveJSProperty("open", true);
  });

  test("the apps index is a simple store: six tiles, 22 rows, no filters, no jargon", async ({
    page,
  }) => {
    const s = messages.en.apps.store;
    // One short plain sentence: no numbers, at most 14 words.
    expect(messages.en.apps.lead.split(/\s+/).length).toBeLessThanOrEqual(14);

    await page.goto(`${base()}/apps/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Apps");
    await expect(page.locator(".store-lead")).toHaveText(messages.en.apps.lead);
    expect(await page.locator(".store-head").innerText()).not.toMatch(/\d/);

    // The built-in apps as tiles, the marketplace apps as rows grouped by category; every tile and
    // row is one link to the app's own page.
    await expect(page.locator("main").getByRole("heading", { level: 2 })).toHaveText([
      s.ready,
      s.more,
    ]);
    await expect(page.locator("main .store-tile")).toHaveCount(6);
    await expect(page.locator("main .store-row")).toHaveCount(22);
    await expect(page.locator("main a[href^='/apps/']")).toHaveCount(28);
    await expect(page.locator("main .store-tile").first()).toHaveAttribute("href", "/apps/crm/");
    // The six tile taglines are plain words, not a list of nouns from the manifests.
    await expect(page.locator("main .store-tile .store-tag")).toHaveText([
      messages.en.apps.taglines["crm"]!,
      messages.en.apps.taglines["sales"]!,
      messages.en.apps.taglines["invoicing"]!,
      messages.en.apps.taglines["product"]!,
      messages.en.apps.taglines["base"]!,
      messages.en.apps.taglines["connectors"]!,
    ]);
    // A row opens a page, so it ends in a chevron, not in a "Get" pill that would promise an install.
    await expect(page.locator("main .store-row .store-chev")).toHaveCount(22);
    await expect(page.locator("main .store-get")).toHaveCount(0);
    expect(await page.locator("main h3").allTextContents()).toEqual(
      Object.values(messages.en.apps.categories).filter((c) => c !== "Foundation"),
    );

    // No filters, counts or status pills, and none of the technical words.
    await expect(page.locator("main input, main fieldset, main .status")).toHaveCount(0);
    const text = (await page.locator("main").innerText()).toLowerCase();
    for (const word of ["models", "fields", "data class", "workflow", "agent"])
      expect(text, word).not.toContain(word);

    await page.locator("main .store-row", { hasText: "Helpdesk" }).click();
    await expect(page).toHaveURL(`${base()}/apps/helpdesk/`);
  });

  test("the home page lists apps in two labelled groups and the status filter shows one", async ({
    page,
  }) => {
    await page.goto(`${base()}/`);
    await expect(page.locator("#apps .apps-group-title")).toHaveCount(2);
    await page.locator('#apps label[for="st-live"]').click();
    await expect(page.locator("#apps .app:visible")).toHaveCount(6);
    await expect(page.locator("#apps .apps-group-title:visible")).toHaveCount(1);
    await page.locator('#apps label[for="st-market"]').click();
    await expect(page.locator("#apps .app:visible")).toHaveCount(22);
    await expect(page.locator("#apps .apps-group-title:visible")).toHaveCount(1);
    await page.locator('#apps label[for="st-all"]').click();
    await expect(page.locator("#apps .app:visible")).toHaveCount(28);
    await expect(page.locator("#apps .apps-group-title:visible")).toHaveCount(2);
  });

  test("a built-in app page is laid out like a store page, with the technical detail folded", async ({
    page,
  }) => {
    const s = messages.en.apps.store;
    await page.goto(`${base()}/apps/invoicing/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Invoicing");

    // The hero: the name, one line, the category as a quiet line and one primary button; a
    // built-in app is opened.
    await expect(page.locator(".app-tagline")).toHaveText(messages.en.apps.taglines["invoicing"]!);
    await expect(page.locator(".app-category")).toHaveText("Finance");
    const primary = page.locator(".app-hero .app-primary");
    await expect(primary).toHaveText(s.open);
    await expect(primary).toHaveAttribute("href", "/app/");
    await expect(page.locator("main .app-primary")).toHaveCount(1);

    // The facts strip: four cells, from the manifest only.
    const a = messages.en.apps;
    await expect(page.locator(".app-facts dt")).toHaveText([
      a.meta.maturity,
      a.meta.version,
      s.worksWith,
      a.meta.license,
    ]);
    // What it works with is a count (the names are the "Works with" section below the strip).
    await expect(page.locator(".app-facts dd")).toHaveText([
      "Stable",
      "0.2.1",
      "2 apps",
      "Apache-2.0",
    ]);
    await expect(page.locator("#works-with ~ .app-icons a")).toHaveText(["Contacts", "Sales"]);

    // The sections, in order; a marketplace app has a "Get" section where a built-in app has none.
    await expect(page.locator("main > .wrap > section > h2")).toHaveText([
      a.h.purpose,
      s.canDo,
      s.keeps,
      s.safe,
      s.worksWith,
      s.info,
      s.developers,
      a.h.related,
    ]);
    // What it is for: the first sentence large, the rest small and quiet.
    await expect(page.locator(".app-purpose")).toHaveText(
      "Prepare customer invoices as drafts and post them to give each one its gapless legal number.",
    );
    await expect(page.locator(".app-purpose-rest")).toContainText("Posting is the one step");
    // What you can do: one card per workflow action, with the approval mark where it is needed.
    // The thing it works on is part of the heading, so a list of headings tells the cards apart.
    const post = page.locator(".app-card", { hasText: "Post a draft invoice" });
    await expect(post.locator("h3")).toHaveText("Invoice Post");
    await expect(post.locator(".app-card-title")).toHaveText("Post");
    await expect(post).toContainText(s.needsApproval);
    await expect(
      page.locator(".app-card", { hasText: "Cancel a draft invoice" }),
    ).not.toContainText(s.needsApproval);
    await expect(page.locator(".app-safe li")).toHaveCount(1);
    await expect(page.locator(".app-info")).toContainText("SpaceCorps");
    // The information list speaks plainly: a version of obop, not a semver range; a built-in app
    // links to its module, not to the site's snapshot of the manifest.
    await expect(page.locator(".app-info")).toContainText(
      s.compatValue.replace("{version}", "0.2"),
    );
    await expect(page.locator(".app-info")).not.toContainText("^0.2");
    // A built-in app's version moves with obop itself, said so without the word "core".
    await expect(page.locator(".app-info")).toContainText(
      s.sameAsObop.replace("{version}", "0.2.1"),
    );
    await expect(page.locator(".app-info")).not.toContainText(/core/i);
    await expect(page.locator(".app-info dt", { hasText: s.source })).toBeVisible();
    await expect(
      page.locator(".app-info a", { hasText: "crates/modules/invoicing" }),
    ).toHaveAttribute("href", /\/tree\/main\/crates\/modules\/invoicing$/);
    // A built-in app has no Get section: one quiet line under the information says it is included.
    await expect(page.locator(".app-getbox")).toHaveCount(0);
    await expect(page.locator(".app-included")).toHaveText(s.included);

    // For developers: a heading, then a fold that is present and closed, with everything technical
    // inside it, one heading level below the heading.
    await expect(page.locator("h2#for-developers")).toHaveText(s.developers);
    const dev = page.locator("details#developers");
    await expect(dev).toHaveJSProperty("open", false);
    await expect(dev.locator("summary")).toHaveText(s.developersShow);
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeHidden();
    await dev.locator("summary").click();
    await expect(dev).toHaveJSProperty("open", true);
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeVisible();
    await expect(page.getByRole("cell", { name: "total_minor" })).toBeVisible();
    await expect(page.getByRole("cell", { name: "invoice.post" })).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "What agents can and cannot do" }),
    ).toBeVisible();
    await expect(page.getByText("It can only propose these steps")).toContainText("invoice.post");
    // The headings keep the ids they always had, so old links still land.
    for (const id of ["what-it-is-for", "models", "workflows", "install", "build-your-own"])
      await expect(page.locator(`#${id}`)).toHaveCount(1);
  });

  test("a marketplace app page gets: the button jumps to a plain install box", async ({ page }) => {
    const s = messages.en.apps.store;
    await page.goto(`${base()}/apps/helpdesk/`);
    const primary = page.locator(".app-hero .app-primary");
    await expect(primary).toHaveText(s.get);
    await expect(primary).toHaveAttribute("href", "#get");
    await primary.click();
    await expect(page).toHaveURL(/\/apps\/helpdesk\/#get$/);
    await expect(page.getByRole("heading", { level: 2, name: s.get })).toBeInViewport();
    // The box ends the jump with something to do: one plain sentence, the pill that opens obop
    // (where Customize and its Marketplace are), and the command line, shown, with what it needs.
    await expect(page.locator(".app-getbox > p").first()).toHaveText(s.getLead);
    const open = page.locator(".app-getbox a.app-pill");
    await expect(open).toHaveText(s.openObop);
    await expect(open).toHaveAttribute("href", "/app/");
    await expect(page.locator("main .app-primary")).toHaveCount(1); // the one button of the hero
    // The jump lands on a filled pill, not a grey one: with the hero's button off screen it is the
    // one filled pill of that screen, and the header's "Start free" is the plain button.
    const fill = (sel: string) =>
      page.locator(sel).evaluate((el) => {
        const cs = getComputedStyle(el);
        return { background: cs.backgroundColor, color: cs.color, border: cs.borderTopStyle };
      });
    expect(await fill(".app-getbox a.app-pill")).toEqual(await fill(".app-hero .app-primary"));
    await expect(page.locator(".app-hero .app-primary")).not.toBeInViewport();
    expect(await fill(".head-cta .btn.primary")).not.toEqual(await fill(".app-getbox a.app-pill"));
    await expect(page.locator(".app-getbox .app-or")).toHaveText(s.commandLine);
    await expect(page.locator(".app-getbox pre code")).toBeVisible();
    await expect(page.locator(".app-getbox pre code")).toHaveText(
      "obop modules install-from helpdesk",
    );
    await expect(page.locator(".app-getbox .fine")).toHaveText(s.getNote);
    await expect(page.locator(".app-getbox details")).toHaveCount(0);
    await expect(page.locator("details#developers")).toHaveJSProperty("open", false);
    await page.locator("details#developers summary").click();
    await expect(page.getByRole("cell", { name: "requester_email" })).toBeVisible();
    // The same command is also part of the folded technical detail.
    await expect(
      page.locator("pre code").filter({ hasText: "obop modules install-from helpdesk" }),
    ).toHaveCount(2);
  });

  test("an app without workflow actions says records are edited directly", async ({ page }) => {
    await page.goto(`${base()}/apps/products/`);
    await expect(page.locator(".app-direct")).toHaveText(messages.en.apps.store.directEdit);
    await expect(page.locator(".app-card h3")).toHaveText(["Product"]);
  });

  test("a translated app page speaks the page language and marks the English text", async ({
    page,
  }) => {
    const s = messages.de.apps.store;
    await page.goto(`${base()}/de/apps/helpdesk/`);
    await expect(page.locator(".app-hero .app-primary")).toHaveText(s.get);
    await expect(page.locator(".app-tagline")).toHaveText(messages.de.apps.taglines["helpdesk"]!);
    await expect(page.getByRole("heading", { level: 2, name: s.canDo })).toBeVisible();
    // The cards use the labels the product itself has in German; only the purpose is English,
    // marked as such, with one quiet line that says so.
    await expect(page.locator(".app-purpose")).toHaveAttribute("lang", "en");
    await expect(page.locator(".app-note")).toHaveText(s.purposeEnglish);
    await expect(page.locator(".app-card .app-card-title").first()).toHaveText("Übernehmen");
    await expect(page.locator(".app-card [lang]")).toHaveCount(0);
    await expect(page.locator("h2#for-developers")).toHaveText(s.developers);
    await expect(page.locator("details#developers > summary")).toHaveText(s.developersShow);
    // Field descriptions in the technical tables are English and say so.
    await expect(page.locator("details#developers td span[lang=en]").first()).toBeAttached();
    // The note that says so comes right under the heading, before the English it explains.
    expect(
      await page.evaluate(
        () => document.querySelector(".app-note")!.nextElementSibling?.className ?? "",
      ),
    ).toBe("app-purpose");
    await page.goto(`${base()}/de/apps/crm/`);
    await expect(page.locator(".app-hero .app-primary")).toHaveText(s.open);
    await expect(page.locator(".app-hero .app-primary")).toHaveAttribute("href", "/app/");
  });

  test("a translated page calls an app what the product calls it in that language", async ({
    page,
    request,
  }) => {
    // The names are the ones in packages/labels (D52), so the site and the product agree; the URL,
    // the icon and the English names on the English pages do not change.
    await page.goto(`${base()}/sv/apps/`);
    await expect(page.locator(".store-tile .store-name")).toHaveText([
      "CRM",
      "Försäljning",
      "Fakturering",
      "Produkter",
      "Kontakter",
      "Kopplingar",
    ]);
    await expect(page.locator(".store-row", { hasText: "Kundtjänst" })).toHaveAttribute(
      "href",
      "/sv/apps/helpdesk/",
    );
    await page.goto(`${base()}/sv/apps/helpdesk/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Kundtjänst");
    await expect(page).toHaveTitle(/^Kundtjänst: /);
    await page.goto(`${base()}/de/apps/field-service/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(DE_FIELD_SERVICE);
    // Who it works with, the related apps and the copy say the same names; the strip counts them.
    const works = await page.locator("#works-with ~ .app-icons a").count();
    expect(works).toBeGreaterThan(1);
    await expect(page.locator(".app-facts dd").nth(2)).toHaveText(
      messages.de.apps.store.worksCount.other.replace("{n}", String(works)),
    );
    await expect(
      page.locator("#works-with ~ .app-icons a[href='/de/apps/contacts/']"),
    ).toContainText("Kontakte");
    const md = await (await request.get(`${base()}/de/apps/field-service.md`)).text();
    expect(md).toContain("[Kontakte](/de/apps/contacts/)");
    // The English page keeps the English names.
    await page.goto(`${base()}/apps/field-service/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Field Service");
  });

  // What the page shows is checked against the manifest read straight from the TOML file, not
  // through the site's own loader: the cards, the folds, the facts, the safe lines, the icons.
  for (const app of [
    { slug: "accounting", file: "packages/accounting.toml", builtin: false },
    { slug: "connectors", file: "web/apps/site/data/builtin/connectors.toml", builtin: true },
  ]) {
    test(`the ${app.slug} page shows what its manifest says`, async ({ page }) => {
      const s = messages.en.apps.store;
      const toml = readFileSync(join(repoRoot, app.file), "utf8");
      const actionBlocks = toml
        .split(/^\s*\[\[model\.action\]\]/m)
        .slice(1)
        .map((b) => b.split(/\n\s*\[/)[0]!);
      const fieldBlocks = toml
        .split(/^\s*\[\[model\.field\]\]/m)
        .slice(1)
        .map((b) => b.split(/\n\s*\[/)[0]!);
      const models = (toml.match(/^\[\[model\]\]/gm) ?? []).length;
      const approvals = actionBlocks.filter((b) => /^\s*approval\s*=\s*true/m.test(b)).length;
      const withheldClass =
        /^\s*class\s*=\s*"(financial|confidential_hr|special_category|secret)"/m;
      const masked = fieldBlocks.some(
        (b) => /^\s*personal_data\s*=\s*true/m.test(b) && !withheldClass.test(b),
      );
      const withheld = fieldBlocks.some((b) => withheldClass.test(b));
      const version = /^version\s*=\s*"([^"]+)"/m.exec(toml)![1]!;
      const maturity = /^maturity\s*=\s*"([^"]+)"/m.exec(toml)![1]!;
      const deps = [
        ...(/^depends\s*=\s*\[([^\]]*)\]/m.exec(toml)?.[1] ?? "").matchAll(/"([^"]+)"/g),
      ].map((m) => m[1]!);
      for (const m of /\[module\.dependencies\]\n((?:[a-z_]+\s*=.*\n?)*)/
        .exec(toml)?.[1]
        ?.matchAll(/^([a-z_]+)\s*=/gm) ?? [])
        deps.push(m[1]!);
      const names: Record<string, string> = { base: "Contacts" };

      await page.goto(`${base()}/apps/${app.slug}/`);
      // Facts: maturity, version, who it works with, licence.
      const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
      const license = /^license\s*=\s*"([^"]+)"/m.exec(toml)![1]!;
      const dd = await page.locator(".app-facts dd").allTextContents();
      expect(dd[0]).toBe(cap(maturity));
      expect(dd[1]).toBe(version);
      expect(dd[2]).toBe(
        deps.length
          ? (deps.length === 1 ? s.worksCount.one : s.worksCount.other).replace(
              "{n}",
              String(deps.length),
            )
          : s.standalone,
      );
      // The names of those apps are the icons of "Works with", in the manifest's order.
      if (deps.length)
        await expect(page.locator("#works-with ~ .app-icons a")).toHaveText(
          deps.map((d) => names[d] ?? cap(d)),
        );
      expect(dd[3]).toBe(license);
      // What you can do: one card per action, the approval mark exactly where the manifest says,
      // and the cards beyond the sixth folded with a label that counts and names them (a fold is
      // only made for two cards or more).
      const can = page.locator("section", { has: page.locator("h2#can-do") });
      await expect(can.locator(".app-card")).toHaveCount(actionBlocks.length);
      await expect(can.locator(".app-card-note")).toHaveCount(approvals);
      // The sentence is said once, on the first marked card; every other mark is the icon with the
      // same words for screen readers, so the page does not repeat "agent" down its main path.
      // (The last span of a note holds the words; the first holds the icon.)
      const words = can.locator(".app-card-note > span:last-child");
      await expect(words).toHaveText(Array(approvals).fill(s.needsApproval));
      await expect(can.locator(".app-card-note > span:last-child:not(.app-sr)")).toHaveCount(
        approvals ? 1 : 0,
      );
      // The one that is drawn is the first marked card's, whichever fold it sits in.
      if (approvals)
        expect((await words.first().getAttribute("class")) ?? "").not.toContain("app-sr");
      const folded = (n: number) => (n > 7 ? n - 6 : 0);
      const foldedActions = folded(actionBlocks.length);
      if (foldedActions)
        await expect(can.locator(".app-more > summary")).toHaveText(
          s.showMoreActions.replace("{n}", String(foldedActions)),
        );
      else await expect(can.locator(".app-more")).toHaveCount(0);
      // What it keeps: one card per model, the same way.
      const keeps = page.locator("section", { has: page.locator("h2#keeps") });
      await expect(keeps.locator(".app-card")).toHaveCount(models);
      const foldedModels = folded(models);
      if (foldedModels)
        await expect(keeps.locator(".app-more > summary")).toHaveText(
          s.showMoreKeeps.replace("{n}", String(foldedModels)),
        );
      // Safe with AI agents: exactly the lines the manifest earns.
      const lines = [
        ...(masked ? [s.safeMasked] : []),
        ...(withheld ? [s.safeWithheld] : []),
        approvals ? s.safeApprove : s.safeApproveNone,
      ];
      await expect(page.locator(".app-safe li")).toHaveText(lines);
      // Works with: a link for each dependency that is an app of its own.
      if (deps.includes("base"))
        await expect(
          page.locator("#works-with ~ .app-icons a[href='/apps/contacts/']"),
        ).toHaveCount(1);
      await expect(page.locator("main .app-primary")).toHaveAttribute(
        "href",
        app.builtin ? "/app/" : "#get",
      );
    });
  }

  test("what it is for: a short large lead, and a long rest folds", async ({ page }) => {
    const s = messages.en.apps.store;
    // The large lead is one short sentence; a long first sentence is cut at its colon.
    await page.goto(`${base()}/apps/hr/`);
    await expect(page.locator(".app-purpose")).toHaveText(
      "A register of employees, departments and leave requests, with strict data classes.",
    );
    await expect(page.locator(".app-purpose-rest")).toContainText(
      "Pay, bank details and national id",
    );
    await expect(page.locator(".app-read")).toHaveCount(0);
    // A rest that is a wall of text sits under "Read more", closed, and is still in the page.
    await page.goto(`${base()}/apps/connectors/`);
    const read = page.locator("details.app-read");
    await expect(read).toHaveJSProperty("open", false);
    await expect(read.locator("summary")).toHaveText(s.readMore);
    await expect(page.locator(".app-purpose-rest")).toBeHidden();
    expect(await page.locator(".app-purpose").innerText()).toMatch(/^Connect other systems/);
    await read.locator("summary").click();
    await expect(page.locator(".app-purpose-rest")).toContainText("Article 30");
    // The Markdown copy says the whole purpose as one paragraph, as the page does.
    const md = await (await page.request.get(`${base()}/apps/hr.md`)).text();
    expect(md).toContain(
      "with strict data classes. Pay, bank details and national id are visible to managers only",
    );
  });

  test("one or two cards use the width of the column; the others keep their columns", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(`${base()}/apps/invoicing/`);
    const edge = await page.evaluate(() => {
      const list = document.querySelector("#can-do ~ .app-cards")!;
      const cards = [...list.querySelectorAll(".app-card")].map((c) => c.getBoundingClientRect());
      return { cards: cards.length, gap: list.getBoundingClientRect().right - cards.at(-1)!.right };
    });
    expect(edge.cards).toBe(2);
    expect(edge.gap).toBeLessThan(2);
    // Fleet has seven actions: all shown, no fold for a single card.
    await page.goto(`${base()}/apps/fleet/`);
    await expect(page.locator("#can-do ~ .app-cards .app-card")).toHaveCount(7);
    await expect(page.locator("#can-do ~ .app-more")).toHaveCount(0);
  });

  test("English words are not hyphenated; the languages with long compounds hyphenate their own", async ({
    page,
  }) => {
    const hyphens = (sel: string) =>
      page
        .locator(sel)
        .first()
        .evaluate((el) => getComputedStyle(el).hyphens);
    await page.goto(`${base()}/apps/ecommerce/`);
    for (const sel of [".app-card-text", ".app-card h3", ".app-facts dd", ".app-hero h1"])
      expect(await hyphens(sel), sel).toBe("manual");
    await page.goto(`${base()}/de/apps/ecommerce/`);
    for (const sel of [".app-card-text", ".app-card h3", ".app-hero h1", ".app-icons a span"])
      expect(await hyphens(sel), `de ${sel}`).toBe("auto");
    // A short value in the strip ("3 Apps", "Apache-2.0") is never split, in any language.
    expect(await hyphens(".app-facts dd"), "de .app-facts dd").toBe("manual");
    await expect(page.locator(".app-purpose")).toHaveAttribute("lang", "en");
    expect(await hyphens(".app-purpose")).toBe("manual");
  });

  test("the app icons are sized by the page in rem, with squircle corners and no inline pixel size", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(`${base()}/apps/`);
    const icons = await page.evaluate(() => {
      const size = (sel: string) => {
        const el = document.querySelector(sel) as HTMLElement;
        const r = el.getBoundingClientRect();
        return {
          w: r.width,
          inline: el.getAttribute("style") ?? "",
          radius: getComputedStyle(el).borderTopLeftRadius,
        };
      };
      return { tile: size(".store-tile .app-icon"), row: size(".store-row .app-icon") };
    });
    expect([icons.tile.w, icons.row.w]).toEqual([96, 48]);
    for (const i of [icons.tile, icons.row]) {
      expect(i.inline).not.toMatch(/(?:^|;)\s*(?:width|height)\s*:/);
      expect(i.radius).toBe("22.37%"); // the squircle
    }
    await page.goto(`${base()}/apps/invoicing/`);
    expect((await page.locator(".app-hero .app-icon").boundingBox())!.width).toBe(120);
    expect((await page.locator(".app-icons .app-icon").first().boundingBox())!.width).toBe(64);
  });

  test("search still finds the words only the technical part uses, and the facts of every app", async ({
    request,
  }) => {
    const index = (await (await request.get(`${base()}/search/en.json`)).json()) as [
      string,
      string,
      string,
      string,
    ][];
    const apps = index.filter(([url]) => /^\/apps\/[^/]+\/$/.test(url));
    expect(apps).toHaveLength(28);
    const having = (word: string) =>
      apps.filter(([, , text]) => text.toLowerCase().includes(word.toLowerCase())).length;
    // On main these found 1, 1, 3, 28 and 28 apps; the plain layer must not push them out.
    expect(having("website.user")).toBeGreaterThanOrEqual(1);
    expect(having("refunded")).toBeGreaterThanOrEqual(1);
    expect(having("gapless")).toBeGreaterThanOrEqual(3);
    expect(having("Apache-2.0")).toBe(28);
    expect(having("SpaceCorps")).toBe(28);
  });

  test("docs: sidebar, breadcrumb, previous and next, edit link, table of contents", async ({
    page,
  }) => {
    await page.goto(`${base()}/docs/`);
    const side = page.locator(".docs-side").getByRole("navigation", { name: "Documentation" });
    await side.getByRole("link", { name: "Connect over MCP" }).click();
    await expect(page).toHaveURL(`${base()}/docs/agents/mcp/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Connect over MCP");
    await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toContainText(
      "Documentation",
    );
    await expect(page.getByRole("link", { name: "Edit this page on GitHub" })).toHaveAttribute(
      "href",
      /github\.com\/SpaceCorps\/obop\/edit\/main\/web\/apps\/site\/content\/en\/docs\/agents\/mcp\.md/,
    );
    await expect(page.locator("a[rel=prev]")).toContainText("Create an agent token");
    await expect(page.locator("a[rel=next]")).toContainText("Use the REST API");
    await expect(page.getByRole("navigation", { name: "On this page" })).toContainText(
      "MCP over stdio",
    );
    await expect(page.getByRole("heading", { name: "Cursor" }).first()).toBeVisible();
  });

  test("the module manifest reference is rendered from docs/MODULES.md", async ({ page }) => {
    await page.goto(`${base()}/docs/reference/module-manifest/`);
    await expect(
      page.getByRole("heading", { name: /How to write a module|What a module is/ }).first(),
    ).toBeVisible();
    await expect(page.getByText('class = "none"').first()).toBeVisible();
  });

  test("the marketplace lists the 22 bundled packages and how to publish", async ({ page }) => {
    await page.goto(`${base()}/marketplace/`);
    await expect(page.locator("table tbody tr")).toHaveCount(22);
    await expect(page.getByText("index.json").first()).toBeVisible();
  });

  test("changelog, roadmap, trust pages and the brand downloads exist", async ({
    page,
    request,
  }) => {
    for (const [path, h1] of [
      ["/changelog/", "Changelog"],
      ["/roadmap/", "Roadmap"],
      ["/about/", "About"],
      ["/contact/", "Contact"],
      ["/security/", "Security"],
      ["/privacy/", "Privacy"],
      ["/terms/", "Terms"],
      ["/brand/", "Brand"],
      ["/about/community/", "Community"],
      ["/about/status/", "Status"],
      ["/platform/", "Platform"],
      ["/pricing/", "Pricing"],
    ] as const) {
      await page.goto(`${base()}${path}`);
      await expect(page.getByRole("heading", { level: 1 }), path).toHaveText(h1);
    }
    await page.goto(`${base()}/privacy/`);
    await expect(page.getByText("This is a draft.")).toBeVisible();
    await page.goto(`${base()}/changelog/`);
    expect(await page.locator("h2").count()).toBeGreaterThan(0);
    for (const f of [
      "obop-logo.svg",
      "obop-logo-white.svg",
      "obop-mark.svg",
      "obop-wordmark.svg",
    ]) {
      const r = await request.get(`${base()}/brand/${f}`);
      expect(r.status(), f).toBe(200);
      expect(await r.text()).toContain("<svg");
    }
  });

  test("the pricing page says obop is free and labels every hosting price as planned", async ({
    page,
  }) => {
    await page.goto(`${base()}/pricing/`);
    await expect(page.getByText("Planned, not yet offered or charged")).toBeVisible();
    // Self-hosting is free; the six planned servers follow data/hosting.ts (the provider's price
    // times three, rounded up to the next price that ends in 9).
    expect(await page.locator(".plans .price").allTextContents()).toEqual([
      "€0",
      "€29/month",
      "€59/month",
      "€89/month",
      "€139/month",
      "€269/month",
      "€679/month",
    ]);
    await expect(page.locator(".plans article")).toHaveCount(7);
    await expect(page.getByText("Extra NVMe storage: €15 per 50 GB and month.")).toBeVisible();
    // The old paid plans are gone.
    for (const old of ["€49", "€249", "€899"])
      await expect(page.locator(".plans")).not.toContainText(old);
  });

  test("the 404 page is friendly, has search and links, and asks not to be indexed", async ({
    page,
  }) => {
    await page.goto(`${base()}/404.html`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Page not found");
    await expect(page.getByRole("search")).toHaveCount(2); // header and page
    await expect(page.getByRole("link", { name: "Getting started" }).first()).toBeVisible();
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex");
  });

  test("a translated German page shows German text, lang, hreflang and no banner", async ({
    page,
  }) => {
    const de = messages.de;
    await page.goto(`${base()}/de/`);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(de.home.titleA);
    await expect(page.locator('link[rel="alternate"][hreflang="en"]').first()).toHaveAttribute(
      "href",
      /\/$/,
    );
    await expect(page.locator('link[rel="alternate"][hreflang="de"]')).toHaveAttribute(
      "href",
      /\/de\/$/,
    );
    await expect(page.locator('link[rel="alternate"][hreflang="x-default"]')).toHaveAttribute(
      "href",
      new RegExp(`${base().replace(/\./g, "\\.")}|https?://`),
    );
    await expect(page.locator("#apps .app")).toHaveCount(28);
  });

  test("an untranslated page falls back to English with a visible notice and no alternates", async ({
    page,
  }) => {
    await page.goto(`${base()}/de/docs/reference/faq/`);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("note")).toContainText(messages.de.ui.notTranslatedTitle);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("FAQ");
    await expect(page.locator('link[rel="alternate"][hreflang]')).toHaveCount(0);
    await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
      "href",
      /\/docs\/reference\/faq\/$/,
    );
  });
});

test.describe("website behaviour with JavaScript", () => {
  test("search finds a docs page and opens it; the search page works", async ({ page }) => {
    await page.goto(`${base()}/docs/`);
    await page.getByRole("searchbox", { name: "Search" }).first().fill("agent token");
    const hit = page.locator(".search-results a", { hasText: "Create an agent token" }).first();
    await expect(hit).toBeVisible();
    await hit.click();
    await expect(page).toHaveURL(/\/docs\/agents\/tokens\/$/);

    await page.goto(`${base()}/search/?q=masking`);
    await expect(page.locator(".search-list a").first()).toBeVisible();
    await expect(page.locator(".search-list")).toContainText("Masking tokens");
  });

  test("the language switcher lists the seven languages and reaches the German page", async ({
    page,
  }) => {
    await page.goto(`${base()}/docs/agents/mcp/`);
    await page.locator("details.lang summary").click();
    await expect(page.locator("details.lang a")).toHaveCount(7);
    await page.locator("details.lang a", { hasText: "Deutsch" }).click();
    await expect(page).toHaveURL(`${base()}/de/docs/agents/mcp/`);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByRole("heading", { level: 1 })).not.toHaveText("Connect over MCP");
  });

  test("the theme toggle switches and is remembered", async ({ page }) => {
    await page.goto(`${base()}/`);
    const toggle = page.getByRole("button", { name: "Switch between light and dark" });
    await expect(toggle).toBeVisible();
    await toggle.click();
    const first = await page.locator("html").getAttribute("data-theme");
    expect(["light", "dark"]).toContain(first);
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("data-theme", first!);
  });

  test("code blocks have a copy button", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto(`${base()}/docs/getting-started/`);
    const block = page.locator('div[class*="language-"]').first();
    await block.hover();
    await block.getByRole("button", { name: "Copy" }).click();
    await expect(block.getByRole("button", { name: "Copied" })).toBeVisible();
  });

  test("the install box of a marketplace app has the copy button", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto(`${base()}/apps/helpdesk/`);
    const block = page.locator('.app-getbox div[class*="language-"]');
    await block.hover();
    await block.getByRole("button", { name: "Copy" }).click();
    await expect(block.getByRole("button", { name: "Copied" })).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
      "obop modules install-from helpdesk",
    );
  });

  test("a link to a heading inside the folded developer part opens it", async ({ page }) => {
    await page.goto(`${base()}/apps/invoicing/#models`);
    await expect(page.locator("details#developers")).toHaveJSProperty("open", true);
    await expect(page.locator("#models")).toBeInViewport();
    await page.goto(`${base()}/apps/invoicing/`);
    await expect(page.locator("details#developers")).toHaveJSProperty("open", false);
  });

  test("site.js alone opens the folded part for a link to a heading in it", async ({ page }) => {
    // Chrome opens a closed <details> for a fragment link by itself, so this test closes it again
    // and sends a synthetic hashchange, which no browser answers natively: only the script does.
    // The page scrolls smoothly (style.css), and a smooth scroll that is still running would race
    // with the test's own: so smooth scrolling is off for the test (reduced motion turns it off), and
    // the first jump to #models has finished before the fold is closed.
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto(`${base()}/apps/invoicing/#models`);
    await expect(page.locator("#models")).toBeInViewport();
    const dev = page.locator("details#developers");
    await page.evaluate(() => {
      (document.querySelector("details#developers") as HTMLDetailsElement).open = false;
      window.scrollTo({ top: 0, behavior: "instant" });
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    await expect(dev).toHaveJSProperty("open", true);
    await expect(page.locator("#models")).toBeInViewport();
  });

  test("the copy button shows only once site.js has wired it, and has a name in the markup", async ({
    page,
    request,
  }) => {
    const html = await (await request.get(`${base()}/apps/helpdesk/`)).text();
    expect(html).toMatch(/<button class="copy" type="button" aria-label="Copy">/);
    // On every apps page, in every language, each copy button is named in the markup itself (the
    // script that shows it must not be what gives it a name): the install box says "Copy" in the
    // page language, the code blocks of the technical part carry a title.
    for (const locale of Object.keys(messages) as (keyof typeof messages)[])
      for (const url of (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u))) {
        const page_ = await (await request.get(`${base()}${url}`)).text();
        for (const tag of page_.match(/<button[^>]*\bclass="copy"[^>]*>/g) ?? [])
          expect(tag, `${locale} ${url}`).toMatch(/aria-label="[^"]+"|title="[^"]+"/);
        const box = page_.includes('class="app-getbox"');
        if (box)
          expect(page_, `${locale} ${url}`).toContain(
            `<button class="copy" type="button" aria-label="${messages[locale].ui.copy}">`,
          );
      }
    // A script that is served but wires nothing (an aborted request would also log a console error).
    await page.route("**/site/site.js", (route) =>
      route.fulfill({ contentType: "text/javascript", body: "" }),
    );
    await page.goto(`${base()}/apps/helpdesk/`);
    await expect(page.locator(".app-getbox button.copy")).toBeHidden();
  });
});

test.describe("language hint", () => {
  test.use({ locale: "de-DE" });

  test("a German browser sees a dismissible hint on / and is never redirected", async ({
    page,
  }) => {
    await page.goto(`${base()}/`);
    const banner = page.getByRole("region", { name: "Language" });
    await expect(banner).toBeVisible();
    await expect(page).toHaveURL(`${base()}/`);
    await expect(banner.getByRole("link")).toHaveAttribute("href", "/de/");
    await banner.getByRole("button").click();
    await expect(banner).toBeHidden();
    await page.reload();
    await expect(banner).toBeHidden();
  });
});

test.describe("language hint for English browsers", () => {
  test.use({ locale: "en-GB" });
  test("no hint when the browser language is the page language", async ({ page }) => {
    await page.goto(`${base()}/`);
    await expect(page.getByRole("region", { name: "Language" })).toBeHidden();
  });
});

test.describe("crawlers and agents", () => {
  test("sitemap index, per-language sitemaps and robots.txt", async ({ request }) => {
    const index = await (await request.get(`${base()}/sitemap.xml`)).text();
    expect(index).toContain("<sitemapindex");
    for (const l of ["en", "de", "fr", "pl", "fi", "sv", "es"])
      expect(index).toContain(`/sitemap-${l}.xml`);
    expect(index).toContain("/community/sitemap.xml");

    const en = await (await request.get(`${base()}/sitemap-en.xml`)).text();
    expect(en).toContain("xmlns:xhtml");
    expect(en.match(/<url>/g)!.length).toBeGreaterThan(60);
    expect(en).toMatch(/hreflang="x-default"/);

    const robots = await (await request.get(`${base()}/robots.txt`)).text();
    expect(robots).toContain("Sitemap:");
    for (const agent of ["GPTBot", "ClaudeBot", "PerplexityBot", "Google-Extended"])
      expect(robots).toContain(`User-agent: ${agent}`);
    expect(robots).not.toMatch(/^Disallow:\s*\S/m);
  });

  test("llms.txt follows llmstxt.org and links to Markdown that exists", async ({ request }) => {
    const txt = await (await request.get(`${base()}/llms.txt`)).text();
    expect(txt.startsWith("# obop\n")).toBe(true);
    expect(txt).toMatch(/^> .+/m);
    // Only the site's own pages: external links (GitHub) are not served here.
    const links = [...txt.matchAll(/^- \[[^\]]+\]\((https?:\/\/[^)]+)\): .+$/gm)]
      .map((m) => new URL(m[1]!))
      .filter((u) => u.host !== "github.com")
      .map((u) => u.pathname);
    expect(links.length).toBeGreaterThan(40);
    for (const p of links.filter((_, i) => i % 6 === 0)) {
      const r = await request.get(`${base()}${p}`);
      expect(r.status(), p).toBe(200);
    }
    const full = await (await request.get(`${base()}/llms-full.txt`)).text();
    expect(full).toContain("Connect over MCP");
    expect(full.length).toBeGreaterThan(100_000);
  });

  test("every page has a Markdown mirror with front matter", async ({ request }) => {
    const md = await (await request.get(`${base()}/docs/agents/mcp.md`)).text();
    expect(md).toMatch(
      /^---\ntitle: "Connect over MCP"\ndescription: ".+"\ncanonical: "https?:\/\/.+\/docs\/agents\/mcp\/"/,
    );
    expect(md).toContain("# Connect over MCP");
    expect((await request.get(`${base()}/index.md`)).status()).toBe(200);
    expect((await request.get(`${base()}/de/index.md`)).status()).toBe(200);
    expect((await request.get(`${base()}/apps/crm.md`)).status()).toBe(200);
  });

  test("the Markdown copies of the apps index and of an app page match what the pages show", async ({
    request,
  }) => {
    const s = messages.en.apps.store;
    const index = await (await request.get(`${base()}/apps.md`)).text();
    expect(index).toContain(messages.en.apps.lead);
    expect(index).toContain(`## ${s.ready}`);
    expect(index).toContain(`## ${s.more}`);
    expect(index.match(/^- \[/gm)).toHaveLength(28);
    expect(index).not.toContain(`(${messages.en.apps.status.marketplace})`); // no status labels

    const app = await (await request.get(`${base()}/apps/helpdesk.md`)).text();
    for (const h of [s.canDo, s.keeps, s.safe, s.info, s.get, s.developers])
      expect(app, h).toContain(`## ${h}`);
    expect(app).toContain("- **Ticket: close** (" + s.needsApproval + ")");
    expect(app).toContain(s.getLead);
    expect(app).toContain(`[${s.openObop}](/app/)`);
    expect(app).toContain(s.getNote);
    expect(app).toContain("obop modules install-from helpdesk");
    // A built-in app has no Get section, only the line that says it is included.
    const builtin = await (await request.get(`${base()}/apps/invoicing.md`)).text();
    expect(builtin).not.toContain(`## ${s.get}`);
    expect(builtin).toContain(s.included);
    // On a translated page the copy says that the purpose is English, as the page does.
    const sv = await (await request.get(`${base()}/sv/apps/invoicing.md`)).text();
    expect(sv).toContain(messages.sv.apps.store.purposeEnglish);
    expect(sv).toContain("Fakturering");
    expect(app).toContain("### Models");
    expect(app).toContain(`- **${messages.en.apps.meta.version}:** 0.4.0`);
  });

  test("security.txt and humans.txt", async ({ request }) => {
    const sec = await (await request.get(`${base()}/.well-known/security.txt`)).text();
    expect(sec).toMatch(/^Contact: https:\/\/.+$/m);
    expect(sec).toMatch(/^Expires: \d{4}-\d\d-\d\dT/m);
    expect((await request.get(`${base()}/humans.txt`)).status()).toBe(200);
  });

  test("every English page has title, description, canonical, language, one h1 and alternates", async ({
    request,
  }) => {
    const paths = await pagesOf(request, "en");
    expect(paths.length).toBeGreaterThan(60);
    const titles = new Set<string>();
    for (const p of paths) {
      const html = await (await request.get(`${base()}${p}`)).text();
      const title = /<title>([^<]+)<\/title>/.exec(html)?.[1];
      expect(title, `${p} title`).toBeTruthy();
      expect(titles.has(title!), `${p} duplicate title ${title}`).toBe(false);
      titles.add(title!);
      expect(html, `${p} description`).toMatch(/<meta name="description" content="[^"]{30,}/);
      expect(html, `${p} canonical`).toMatch(/<link rel="canonical" href="https?:\/\/[^"]+">/);
      expect(html, `${p} lang`).toMatch(/<html lang="en"/);
      expect((html.match(/<h1[ >]/g) ?? []).length, `${p} h1`).toBe(1);
      expect(html, `${p} og:image`).toContain('property="og:image"');
      expect(html, `${p} markdown alternate`).toMatch(/rel="alternate" type="text\/markdown"/);
      expect(html, `${p} landmarks`).toMatch(/<main[ >]/);
    }
  });

  test("structured data parses and has the types search engines and agents look for", async ({
    page,
  }) => {
    const types = async (path: string) => {
      await page.goto(`${base()}${path}`);
      const raw = await page.locator('script[type="application/ld+json"]').allTextContents();
      expect(raw.length, path).toBeGreaterThan(0);
      return raw.map((r) => JSON.parse(r)["@type"] as string);
    };
    const home = await types("/");
    expect(home).toEqual(
      expect.arrayContaining(["Organization", "WebSite", "SoftwareApplication", "FAQPage"]),
    );
    const site = JSON.parse(
      (await page.locator('script[type="application/ld+json"]').allTextContents()).find((r) =>
        r.includes("WebSite"),
      )!,
    );
    expect(site.potentialAction["@type"]).toBe("SearchAction");
    expect(await types("/apps/crm/")).toEqual(
      expect.arrayContaining(["BreadcrumbList", "SoftwareApplication"]),
    );
    expect(await types("/docs/agents/")).toEqual(
      expect.arrayContaining(["BreadcrumbList", "TechArticle"]),
    );
    expect(await types("/pricing/")).toEqual(expect.arrayContaining(["FAQPage"]));
  });

  test("social cards exist as PNG files", async ({ request }) => {
    for (const p of [
      "/og/en/home.png",
      "/og/en/docs/agents.png",
      "/og/de/home.png",
      "/og/default.png",
      "/apple-touch-icon.png",
    ]) {
      const r = await request.get(`${base()}${p}`);
      expect(r.status(), p).toBe(200);
      const b = await r.body();
      expect(b.subarray(1, 4).toString(), p).toBe("PNG");
    }
  });
});

test.describe("accessibility", () => {
  const samples = [
    "/",
    "/apps/",
    "/apps/crm/",
    "/apps/helpdesk/",
    "/docs/",
    "/docs/agents/mcp/",
    "/pricing/",
    "/search/",
    "/404.html",
    "/de/",
    "/fi/apps/",
    "/sv/apps/invoicing/",
    "/apps/accounting/",
    "/pl/apps/manufacturing/",
    "/marketplace/",
  ];

  test("sample pages: named controls, heading order, landmarks, unique ids, image alt, language", async ({
    page,
  }) => {
    for (const path of samples) {
      await page.goto(`${base()}${path}`);
      expect(await unnamedControls(page, { strict: true }), `${path} controls`).toEqual([]);
      const problems = await page.evaluate(() => {
        const out: string[] = [];
        if (!document.documentElement.lang) out.push("no lang");
        const heads = [...document.querySelectorAll("h1,h2,h3,h4,h5,h6")].map((h) =>
          Number(h.tagName[1]),
        );
        if (heads[0] !== 1) out.push(`first heading is h${heads[0]}`);
        for (let i = 1; i < heads.length; i++)
          if (heads[i]! - heads[i - 1]! > 1)
            out.push(`heading jumps h${heads[i - 1]} to h${heads[i]}`);
        for (const sel of ["header", "nav", "main", "footer"])
          if (!document.querySelector(sel)) out.push(`no <${sel}>`);
        const ids = new Map<string, number>();
        document.querySelectorAll("[id]").forEach((e) => ids.set(e.id, (ids.get(e.id) ?? 0) + 1));
        for (const [id, n] of ids) if (n > 1) out.push(`duplicate id ${id}`);
        document.querySelectorAll("img").forEach((i) => {
          if (!i.hasAttribute("alt")) out.push("img without alt");
        });
        document.querySelectorAll("svg[role=img]").forEach((s) => {
          if (!s.getAttribute("aria-label")) out.push("svg role=img without label");
        });
        const unlabeledNav = [...document.querySelectorAll("nav")].filter(
          (n) => !n.getAttribute("aria-label") && !n.getAttribute("aria-labelledby"),
        );
        if (unlabeledNav.length) out.push("nav without a label");
        return out;
      });
      expect(problems, path).toEqual([]);
    }
  });

  test("every apps page has the same sound structure, in its own language, with one small script", async ({
    page,
    request,
  }) => {
    test.setTimeout(240_000);
    let checked = 0;
    for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
      const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/(.+\/)?$/.test(u));
      expect(urls.length, `${locale} apps pages`).toBe(29);
      for (const url of urls) {
        await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
        const problems = await page.evaluate((htmlLang) => {
          const out: string[] = [];
          if (document.documentElement.lang !== htmlLang)
            out.push(`html lang ${document.documentElement.lang}, not ${htmlLang}`);
          const heads = [...document.querySelectorAll("h1,h2,h3,h4,h5,h6")].map((h) =>
            Number(h.tagName[1]),
          );
          if (heads.filter((h) => h === 1).length !== 1) out.push("not exactly one h1");
          if (heads[0] !== 1) out.push(`first heading is h${heads[0]}`);
          for (let i = 1; i < heads.length; i++)
            if (heads[i]! - heads[i - 1]! > 1)
              out.push(`heading jumps h${heads[i - 1]} to h${heads[i]}`);
          const ids = new Map<string, number>();
          document.querySelectorAll("[id]").forEach((e) => ids.set(e.id, (ids.get(e.id) ?? 0) + 1));
          for (const [id, n] of ids) if (n > 1) out.push(`duplicate id ${id}`);
          for (const n of document.querySelectorAll("nav"))
            if (!n.getAttribute("aria-label") && !n.getAttribute("aria-labelledby"))
              out.push("nav without a label");
          const control = "a[href], button, input, select, textarea, summary";
          for (const el of document.querySelectorAll(control))
            if (el.parentElement?.closest("a[href], button"))
              out.push(`nested control ${el.outerHTML.slice(0, 60)}`);
          for (const el of document.querySelectorAll("main [lang]"))
            if (el.getAttribute("lang") !== "en")
              out.push(`unexpected lang ${el.getAttribute("lang")} on ${el.tagName}`);
          for (const sc of document.querySelectorAll("script[src]"))
            if (sc.getAttribute("src") !== "/site/site.js")
              out.push(`script ${sc.getAttribute("src")}`);
          if (document.querySelector("script[type=module]")) out.push("module script");
          // The English that a translated page carries is marked as English, on every page: the
          // purpose (the lead and the rest), the package text and the descriptions in the field tables.
          if (htmlLang !== "en" && document.querySelector(".app-page")) {
            if (document.querySelector(".app-purpose")?.getAttribute("lang") !== "en")
              out.push("the purpose is not marked lang=en");
            for (const rest of document.querySelectorAll(".app-purpose-rest"))
              if (rest.getAttribute("lang") !== "en")
                out.push("the rest of the purpose is not marked lang=en");
            const notes = document.querySelectorAll("#developers .en-note");
            if (notes.length === 0) out.push("no English note in the technical part");
            for (const n of notes)
              if (n.getAttribute("lang") !== "en")
                out.push("an English note is not marked lang=en");
            for (const t of document.querySelectorAll("#developers table")) {
              if (t.querySelectorAll("thead th").length !== 7) continue; // the field tables
              for (const td of t.querySelectorAll("tbody tr td:last-child"))
                if (td.querySelector(':scope > span[lang="en"]')?.textContent !== td.textContent)
                  out.push(
                    `a field description is not marked lang=en: ${td.textContent?.slice(0, 30)}`,
                  );
            }
          }
          return out;
        }, messages[locale].htmlLang);
        expect(problems, `${locale} ${url}`).toEqual([]);
        // Every control has a name, on every apps page (strict: text under aria-hidden is no name).
        expect(
          await unnamedControls(page, { strict: true }),
          `${locale} ${url} unnamed controls`,
        ).toEqual([]);
        checked++;
      }
    }
    expect(checked).toBe(7 * 29);
  });

  test("the apps pages show a focus ring on every stop of a Tab walk, forward and back, and the focused control is never under the header", async ({
    page,
  }) => {
    test.setTimeout(240_000);
    for (const width of [1440, 375]) {
      await page.setViewportSize({ width, height: 900 });
      for (const [path, atLeast] of [
        ["/apps/", 28],
        ["/apps/helpdesk/", 8],
        ["/apps/accounting/", 11],
      ] as const) {
        for (const key of ["Tab", "Shift+Tab"] as const) {
          await page.goto(`${base()}${path}`);
          let stops = 0;
          for (let i = 0; i < 80; i++) {
            await page.keyboard.press(key);
            // The browser scrolls the focused control into view, smoothly where the page asks for it:
            // measure where it comes to rest, not where it passes.
            await page.evaluate(
              () =>
                new Promise<void>((done) => {
                  let last = -1;
                  let still = 0;
                  const tick = () => {
                    still = window.scrollY === last ? still + 1 : 0;
                    last = window.scrollY;
                    if (still >= 3) done();
                    else requestAnimationFrame(tick);
                  };
                  tick();
                }),
            );
            const stop = await page.evaluate(() => {
              const el = document.activeElement as HTMLElement | null;
              if (!el || !el.closest("main")) return null;
              const cs = getComputedStyle(el);
              const r = el.getBoundingClientRect();
              // The ring is drawn outside the box by its offset and its width.
              const reach = parseFloat(cs.outlineOffset) + parseFloat(cs.outlineWidth);
              return {
                what: `${el.tagName} ${(el.textContent ?? "").trim().slice(0, 30)}`,
                style: cs.outlineStyle,
                width: parseFloat(cs.outlineWidth),
                left: r.left - reach,
                right: r.right + reach,
                top: r.top,
                header: document.querySelector(".site-header")!.getBoundingClientRect().bottom,
                screen: document.documentElement.clientWidth,
              };
            });
            if (!stop) continue;
            stops++;
            const where = `${path} at ${width}px (${key}): ${stop.what}`;
            expect(stop.style, where).not.toBe("none");
            expect(stop.width, where).toBeGreaterThanOrEqual(2);
            expect(stop.left, `${where} ring starts on screen`).toBeGreaterThanOrEqual(0);
            expect(stop.right, `${where} ring ends on screen`).toBeLessThanOrEqual(stop.screen);
            // Going back scrolls a control in from above: the sticky header must not cover it.
            expect(stop.top, `${where} is under the header`).toBeGreaterThanOrEqual(
              stop.header - 0.5,
            );
          }
          expect(stops, `${path} stops in main (${key})`).toBeGreaterThanOrEqual(atLeast);
        }
      }
    }
  });

  test("no tabindex above zero on the apps pages", async ({ page }) => {
    for (const path of ["/apps/", "/apps/helpdesk/", "/apps/accounting/", "/sv/apps/hr/"]) {
      await page.goto(`${base()}${path}`);
      const positive = await page.evaluate(() =>
        [...document.querySelectorAll("[tabindex]")]
          .filter((e) => Number(e.getAttribute("tabindex")) > 0)
          .map((e) => e.outerHTML.slice(0, 60)),
      );
      expect(positive, path).toEqual([]);
    }
  });

  test("with reduced motion the apps pages do not animate", async ({ page }) => {
    const durations = () =>
      page.evaluate(() => {
        const out: Record<string, number> = {};
        for (const sel of [
          ".store-tile",
          ".store-chev",
          ".app-primary",
          ".app-pill",
          ".app-more > summary",
          ".app-dev > summary",
          ".app-dev > summary::after",
        ]) {
          const [base, pseudo] = sel.split("::");
          const el = document.querySelector(base!);
          if (!el) continue;
          const cs = getComputedStyle(el, pseudo ? `::${pseudo}` : null);
          out[sel] = Math.max(
            ...cs.transitionDuration
              .split(",")
              .map((d) => parseFloat(d) * (d.includes("ms") ? 1 : 1000)),
          );
        }
        return out;
      });
    // Not vacuous: without the preference something does transition.
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.goto(`${base()}/apps/`);
    expect(Math.max(...Object.values(await durations()))).toBeGreaterThan(50);
    for (const path of ["/apps/", "/apps/helpdesk/", "/apps/accounting/"]) {
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.goto(`${base()}${path}`);
      const d = await durations();
      for (const [sel, ms] of Object.entries(d)) expect(ms, `${path} ${sel}`).toBeLessThan(50);
    }
  });

  test("the apps CSS uses logical properties only", async () => {
    // RTL is out of scope, but adding it must be a layout review, not a rewrite (SITE.md).
    const physical =
      /\b(?:margin|padding|border)-(?:left|right|top|bottom)\b|(?:^|[\s;{])(?:left|right|top|bottom)\s*:|(?:^|[\s;{])(?:min-|max-)?(?:width|height)\s*:|text-align\s*:\s*(?:left|right)|\bfloat\s*:\s*(?:left|right)/;
    for (const file of ["apps.css", "app-page.css"]) {
      const css = readFileSync(
        join(repoRoot, "web/apps/site/.vitepress/theme", file),
        "utf8",
      ).replace(/\/\*[\s\S]*?\*\//g, "");
      const bad = css.split("\n").filter((l) => physical.test(l));
      expect(bad, file).toEqual([]);
    }
  });

  test("on a touch screen every control the apps pages add is at least 44 px", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
      hasTouch: true,
      isMobile: true,
    });
    const page = await context.newPage();
    const small: string[] = [];
    for (const path of ["/apps/", "/apps/helpdesk/", "/apps/invoicing/", "/apps/accounting/"]) {
      await page.goto(`${base()}${path}`);
      small.push(
        ...(await page.evaluate((p) => {
          const out: string[] = [];
          for (const el of document.querySelectorAll("main a, main summary, main button")) {
            // Inside a closed fold (the technical part) nothing can be tapped.
            const fold = [
              ...(function* () {
                for (let n = el.parentElement; n; n = n.parentElement) yield n;
              })(),
            ].find((n) => n.tagName === "DETAILS" && !(n as HTMLDetailsElement).open);
            if (fold) continue;
            const r = el.getBoundingClientRect();
            if (r.width === 0 || r.height === 0) continue;
            if (r.height < 43.9 || r.width < 43.9)
              out.push(
                `${p} ${el.tagName} "${(el.textContent ?? "").trim().slice(0, 24)}" ${Math.round(r.width)}x${r.height.toFixed(1)}`,
              );
          }
          return out;
        }, path)),
      );
    }
    await context.close();
    expect(small).toEqual([]);
  });

  test("the app hero keeps its size on a phone and stacks at a very large text size", async ({
    page,
    browser,
  }) => {
    // 390 and 375 (and 360) are the phones that matter: the 96 px icon with the name under it, at
    // 2.25rem (36 px; a little less where the screen is narrower than 375 px).
    for (const width of [390, 375, 360]) {
      await page.setViewportSize({ width, height: 812 });
      await page.goto(`${base()}/apps/invoicing/`);
      const hero = await page.evaluate(() => {
        const icon = document.querySelector(".app-hero .app-icon")!.getBoundingClientRect();
        const text = document.querySelector(".app-hero-text")!.getBoundingClientRect();
        const pill = document.querySelector(".app-primary")!.getBoundingClientRect();
        return {
          icon: [icon.width, icon.height],
          h1: parseFloat(getComputedStyle(document.querySelector(".app-hero h1")!).fontSize),
          radius: getComputedStyle(document.querySelector(".app-hero .app-icon")!)
            .borderTopLeftRadius,
          // The name has the whole column: the icon is above it, not beside it.
          textBelowIcon: text.top >= icon.bottom - 0.5,
          textWidth: text.width,
          pillOnFirstScreen: pill.bottom <= window.innerHeight,
        };
      });
      expect(hero.icon, `icon at ${width}px`).toEqual([96, 96]);
      expect(hero.h1, `name at ${width}px`).toBeGreaterThanOrEqual(width >= 375 ? 36 : 34);
      expect(hero.radius).toBe("22.37%"); // the squircle
      expect(hero.textBelowIcon, `the name is under the icon at ${width}px`).toBe(true);
      expect(hero.textWidth, `the name has the column at ${width}px`).toBeGreaterThan(width - 40);
      expect(hero.pillOnFirstScreen, `the button is on the first screen at ${width}px`).toBe(true);
    }
    await page.setViewportSize({ width: 320, height: 700 });
    await page.goto(`${base()}/apps/invoicing/`);
    expect((await page.locator(".app-hero .app-icon").boundingBox())!.width).toBe(72);

    // A large default text size (a browser setting: rem and rem media queries scale) must not push
    // the button off the screen or break the name in the middle of a word.
    for (const [width, font] of [
      [390, 32],
      [390, 28],
      [360, 28],
      [414, 32],
      [320, 32],
    ] as const) {
      const context = await browser.newContext({ viewport: { width, height: 844 } });
      const p = await context.newPage();
      const cdp = await context.newCDPSession(p);
      await cdp.send("Page.setFontSizes", { fontSizes: { standard: font, fixed: 13 } });
      // At 200 percent every language's long words are tried, on pages with long labels and values.
      const paths = [
        "/apps/accounting/",
        "/apps/helpdesk/",
        // The narrowest screen at 200 percent: the folds ("Technische Details anzeigen") and the icon
        // labels of the languages with the longest words must wrap inside their pills, not leave the screen.
        ...(font === 32 && width === 320
          ? [
              "/de/apps/accounting/",
              "/fr/apps/accounting/",
              "/pl/apps/accounting/",
              "/fi/apps/email-marketing/",
              "/fi/apps/website/",
              "/pl/apps/projects/",
              "/pl/apps/hr/",
            ]
          : []),
        ...(font === 32 && width === 390
          ? (Object.keys(messages) as (keyof typeof messages)[]).flatMap((l) =>
              ["field-service", "manufacturing", "hr"].map(
                (a) => `${l === "en" ? "" : `/${l}`}/apps/${a}/`,
              ),
            )
          : ["/de/apps/field-service/"]),
      ];
      for (const path of paths) {
        await p.goto(`${base()}${path}`);
        const over = await p.evaluate(() => {
          // The page's own content (the shared header has its own limits at such sizes).
          const root = document.querySelector(".app-page") as HTMLElement;
          const screen = document.documentElement.clientWidth;
          // A missing button or name is a failed test with a message, not a script error.
          const pillEl = document.querySelector(".app-primary");
          const h1 = document.querySelector(".app-hero h1");
          if (!pillEl || !h1) return null;
          const pill = pillEl.getBoundingClientRect();
          const range = document.createRange();
          range.selectNodeContents(h1);
          const lines = new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
          return {
            page: root.scrollWidth - root.clientWidth,
            pillEnd: Math.round(pill.right) - screen,
            // Every fold and button of the page: "Show technical details" wraps inside its pill.
            folds: [...document.querySelectorAll("main summary, main .app-pill")]
              .filter((e) => e.getBoundingClientRect().right > screen + 0.5)
              .map((e) => (e.textContent ?? "").trim().slice(0, 30)),
            pageEnd: root.scrollWidth - screen,
            words: (h1.textContent ?? "").trim().split(/\s+/).length,
            lines,
          };
        });
        const where = `${path} at ${width}px with ${font}px text`;
        if (!over) throw new Error(`${where}: no hero name (h1) or no primary button`);
        expect(over.page, `${where}: the page's content scrolls sideways`).toBeLessThanOrEqual(0);
        expect(over.pageEnd, `${where}: the content runs past the screen`).toBeLessThanOrEqual(0);
        expect(over.pillEnd, `${where}: the button leaves the screen`).toBeLessThanOrEqual(0);
        expect(over.folds, `${where}: a fold or pill leaves the screen`).toEqual([]);
        if (path === "/apps/accounting/" && width > 320)
          expect(over.lines, `${where}: the name breaks inside the word`).toBe(over.words);
      }
      await context.close();
    }
  });

  test("every text on the apps pages keeps 4.5:1 or better, in both colour schemes and when hovered", async ({
    page,
  }) => {
    const pages = [
      "/apps/",
      "/apps/helpdesk/",
      "/apps/accounting/",
      "/apps/connectors/",
      "/de/apps/helpdesk/",
      "/fi/apps/",
    ];
    // The controls whose colours change on hover: the folds, the two pills and the "Read more" fold.
    const hover = [
      ".app-more > summary",
      ".app-dev > summary",
      ".app-read > summary",
      ".app-primary",
      ".app-getbox .app-pill",
    ];
    let walked = 0;
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      for (const path of pages) {
        await page.goto(`${base()}${path}`);
        const all = (await contrast(page, null))!;
        walked += all.texts;
        // The page has real text to measure, so a walk that finds nothing cannot pass.
        expect(all.texts, `${scheme} ${path} text nodes`).toBeGreaterThan(20);
        expect(
          all.worst[0]!.ratio,
          `${scheme} ${path} lowest: ${JSON.stringify(all.worst)}`,
        ).toBeGreaterThanOrEqual(4.5);
        await page.reload();
        for (const sel of hover) {
          if (!(await page.locator(sel).count())) continue;
          const rest = (await contrast(page, sel))!;
          expect(rest.worst[0]!.ratio, `${scheme} ${path} ${sel}`).toBeGreaterThanOrEqual(4.5);
          await page.locator(sel).first().hover();
          await page.waitForTimeout(250);
          const hovered = (await contrast(page, sel))!;
          expect(
            hovered.worst[0]!.ratio,
            `${scheme} ${path} ${sel} hovered`,
          ).toBeGreaterThanOrEqual(4.5);
          await page.mouse.move(0, 0);
        }
      }
    }
    expect(walked).toBeGreaterThan(2000);
  });

  test("the contrast walk finds a pale text that no list of selectors names", async ({ page }) => {
    await page.goto(`${base()}/apps/helpdesk/`);
    const before = (await contrast(page, null))!;
    expect(before.worst[0]!.ratio).toBeGreaterThanOrEqual(4.5);
    await page.reload();
    // About 1.6:1 on white: the style of "Safe with AI agents" made pale.
    await page.addStyleTag({ content: ".app-safe li { color: #c4c7cf !important; }" });
    const after = (await contrast(page, null))!;
    expect(after.worst[0]!.ratio).toBeLessThan(2);
    expect(after.worst[0]!.at).toContain("li");
  });

  test("the pill buttons keep an edge in forced colours, and a printed page keeps the technical part", async ({
    page,
  }) => {
    await page.emulateMedia({ forcedColors: "active" });
    // Every pill the apps pages draw: the hero's button, the Get box's button, the folds.
    for (const [path, selectors] of [
      [
        "/apps/accounting/",
        [".app-primary", ".app-getbox .app-pill", ".app-more > summary", ".app-dev > summary"],
      ],
      ["/apps/connectors/", [".app-read > summary", ".app-primary"]],
    ] as const) {
      await page.goto(`${base()}${path}`);
      for (const sel of selectors) {
        const border = await page
          .locator(sel)
          .first()
          .evaluate((el) => {
            const cs = getComputedStyle(el);
            return { style: cs.borderTopStyle, width: parseFloat(cs.borderTopWidth) };
          });
        expect(border, `${path} ${sel}`).toEqual({ style: "solid", width: 1 });
      }
    }
    // Printed (at the width of an A4 page): the folds print open, the tables fit the paper, and
    // neither the hero's button nor the copy buttons are printed.
    await page.setViewportSize({ width: 794, height: 1123 });
    await page.goto(`${base()}/apps/accounting/`);
    await page.emulateMedia({ forcedColors: "none", media: "print" });
    await expect(page.locator(".app-primary")).toBeHidden();
    await expect(page.locator("button.copy").first()).toBeHidden();
    await expect(page.locator("details#developers")).toHaveJSProperty("open", false);
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeVisible();
    const cut = await page.evaluate(() =>
      [...document.querySelectorAll("#developers table")]
        .filter((t) => t.scrollWidth > t.clientWidth + 1)
        .map((t) => `${t.scrollWidth}>${t.clientWidth}`),
    );
    expect(cut, "tables cut at the edge of the paper").toEqual([]);
  });

  test("the apps index does not overflow at a large text size on a narrow screen", async ({
    page,
  }) => {
    for (const [width, size] of [
      [640, 32],
      [700, 32],
      [390, 24],
    ] as const) {
      await page.setViewportSize({ width, height: 900 });
      for (const path of ["/apps/", "/de/apps/", "/fi/apps/"]) {
        await page.goto(`${base()}${path}`);
        await page.addStyleTag({ content: `html { font-size: ${size}px !important; }` });
        // The page's own content (the shared header has its own limits at such sizes).
        const over = await page.evaluate(() => {
          const store = document.querySelector(".store") as HTMLElement;
          const clipped = [...document.querySelectorAll(".store-text")].filter(
            (e) => e.scrollWidth > e.clientWidth + 1,
          ).length;
          return { page: store.scrollWidth - store.clientWidth, clipped };
        });
        expect(over, `${path} at ${width}px with ${size}px text`).toEqual({ page: 0, clipped: 0 });
      }
    }
  });

  test("no horizontal scroll at 390px and 320px", async ({ page }) => {
    for (const w of [390, 320]) {
      await page.setViewportSize({ width: w, height: 800 });
      for (const path of [
        "/",
        "/apps/",
        "/docs/agents/mcp/",
        "/apps/helpdesk/",
        "/apps/invoicing/",
        "/de/apps/connectors/",
      ]) {
        await page.goto(`${base()}${path}`);
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow, `${path} at ${w}px`).toBeLessThanOrEqual(0);
      }
    }
  });

  test("no name, label or value is cut in the middle of a word on a phone, in any language", async ({
    page,
    request,
  }) => {
    test.setTimeout(300_000);
    let checked = 0;
    for (const locale of Object.keys(messages)) {
      const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u));
      expect(urls.length, `${locale} app pages`).toBe(28);
      for (const url of urls) {
        await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
        // The safety nets (break-word, hyphens) are switched off: a word that does not fit its box
        // then shows as overflow, instead of being broken in the middle by the browser.
        await page.addStyleTag({
          content:
            "* { overflow-wrap: normal !important; hyphens: manual !important; word-break: normal !important; }",
        });
        for (const width of [320, 375, 768]) {
          await page.setViewportSize({ width, height: 800 });
          const cut = await page.evaluate(() =>
            [
              ...document.querySelectorAll(
                ".app-hero h1, .app-icons a, .app-info dt, .app-facts dt, .app-facts dd",
              ),
            ]
              .filter((e) => e.scrollWidth > e.clientWidth + 1)
              .map((e) => `${e.tagName} "${(e.textContent ?? "").trim().slice(0, 30)}"`),
          );
          expect(cut, `${url} at ${width}px`).toEqual([]);
        }
        checked++;
      }
    }
    expect(checked).toBe(7 * 28);
  });

  test("the Markdown copy of every app page says what the page says, in every language", async ({
    page,
    request,
  }) => {
    test.setTimeout(300_000);
    let checked = 0;
    for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
      const needsApproval = messages[locale].apps.store.needsApproval;
      const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u));
      for (const url of urls) {
        await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
        const view = await page.evaluate(() => {
          const t = (e: Element | null) => (e?.textContent ?? "").replace(/\s+/g, " ").trim();
          const cards = (heading: string) =>
            [
              ...document.querySelectorAll(
                `${heading} ~ ul.app-cards > li.app-card, ${heading} ~ .app-more li.app-card`,
              ),
            ].map((c) => ({
              kicker: t(c.querySelector(".app-card-kicker")),
              title: t(c.querySelector(".app-card-title")),
              text: t(c.querySelector(".app-card-text")),
              approval: !!c.querySelector(".app-card-note"),
            }));
          const icons = (section: Element | undefined) =>
            [...(section?.querySelectorAll(".app-icons a") ?? [])].map((a) => ({
              name: t(a),
              href: a.getAttribute("href"),
            }));
          const sections = [...document.querySelectorAll(".app-page > section")];
          return {
            facts: [...document.querySelectorAll(".app-facts > div")].map((d) => ({
              label: t(d.querySelector("dt")),
              value: t(d.querySelector("dd")),
            })),
            actions: cards("#can-do"),
            models: cards("#keeps"),
            safe: [...document.querySelectorAll(".app-safe li")].map((li) => t(li)),
            works: icons(sections.find((s) => s.querySelector("#works-with"))),
            related: icons(
              sections.find(
                (s) => s.querySelector(".app-icons") && !s.querySelector("#works-with"),
              ),
            ),
            info: [...document.querySelectorAll(".app-info > div")].map((d) => ({
              label: t(d.querySelector("dt")),
              value: t(d.querySelector("dd")),
            })),
          };
        });
        // A no-break space (French puts one before a semicolon) reads as a space on the page.
        const md = (
          await (await request.get(`${base()}${url.replace(/\/$/, "")}.md`)).text()
        ).replace(/[  ]/g, " ");
        const lines = md.split("\n");
        const where = `${locale} ${url}`;
        // The strip is the first list of the copy, line for line (the Information list repeats some of
        // its labels and values further down, so a line that is merely contained would not do).
        const first = lines.findIndex((l) => l.startsWith("- **"));
        const strip: string[] = [];
        for (let i = first; first >= 0 && lines[i]?.startsWith("- **"); i++) strip.push(lines[i]!);
        expect(strip, `${where} facts`).toEqual(
          view.facts.map((f) => `- **${f.label}:** ${f.value}`),
        );
        for (const c of view.actions)
          expect(
            lines.some(
              (l) =>
                l.startsWith(`- **${c.kicker}: `) &&
                l.toLowerCase().includes(c.title.toLowerCase()) &&
                l.endsWith(`: ${c.text}`) &&
                l.includes(needsApproval) === c.approval,
            ),
            `${where} action ${c.kicker} ${c.title}`,
          ).toBe(true);
        for (const c of view.models)
          expect(lines, `${where} keeps`).toContain(`- **${c.title}**: ${c.text}`);
        for (const line of view.safe) expect(lines, `${where} safe`).toContain(`- ${line}`);
        for (const a of [...view.works, ...view.related])
          expect(md, `${where} link`).toContain(`[${a.name}](${a.href})`);
        // The lists are as long as the page's, not only contained in it.
        const section = (heading: string) => {
          const at = lines.indexOf(heading);
          if (at < 0) return [];
          const list: string[] = [];
          for (let i = at + 2; i < lines.length && lines[i]!.startsWith("- "); i++)
            list.push(lines[i]!);
          return list;
        };
        const listed = (heading: string) => section(heading).length;
        expect(listed(`## ${messages[locale].apps.store.worksWith}`), `${where} works with`).toBe(
          view.works.length,
        );
        expect(listed(`## ${messages[locale].apps.h.related}`), `${where} related`).toBe(
          view.related.length,
        );
        expect(listed(`## ${messages[locale].apps.store.info}`), `${where} information`).toBe(
          view.info.length,
        );
        for (const r of view.info) {
          // The row of the Information section (the strip above repeats some labels).
          const line = section(`## ${messages[locale].apps.store.info}`).find((l) =>
            l.startsWith(`- **${r.label}:**`),
          );
          expect(line, `${where} information ${r.label}`).toBeDefined();
          expect(line, `${where} information ${r.label}`).toContain(r.value);
        }
        checked++;
      }
    }
    expect(checked).toBe(7 * 28);
  });

  test("no VitePress runtime is left in the files that are deployed", async () => {
    // transformHtml takes the framework script out of every page; postbuild takes the files out of
    // the output, so nothing dead is served (the pages load /site/site.js and nothing from here).
    const assets = join(siteDir, "assets");
    expect(readdirSync(assets).filter((f) => f.endsWith(".js"))).toEqual([]);
    expect(existsSync(join(assets, "chunks"))).toBe(false);
  });

  test("pages are usable with the keyboard: skip link first, then the header", async ({ page }) => {
    await page.goto(`${base()}/docs/`);
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: "Skip to the content" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#main$/);
  });
});

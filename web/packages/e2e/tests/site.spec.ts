// The public website (web/apps/site, served at the root of the control plane): static HTML that
// must read and navigate without JavaScript, be well-formed for crawlers and agents, and be
// localised. Runs against the built web/site directory.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { messages } from "../../../apps/site/i18n/index.ts";
import { platformUrl, repoRoot } from "../support/env.ts";
import { expect, test, unnamedControls } from "../support/fixtures.ts";

const base = () => platformUrl();

/** URLs of every page of one language, from its sitemap. */
async function pagesOf(request: import("@playwright/test").APIRequestContext, locale: string) {
  const xml = await (await request.get(`${base()}/sitemap-${locale}.xml`)).text();
  return [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => new URL(m[1]!).pathname);
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

    // The hero: the name, one line, and one primary button; a built-in app is opened. The category
    // is in the strip, not repeated under the tagline.
    await expect(page.locator(".app-tagline")).toHaveText(messages.en.apps.taglines["invoicing"]!);
    await expect(page.locator(".app-category")).toHaveCount(0);
    const primary = page.locator(".app-hero .app-primary");
    await expect(primary).toHaveText(s.open);
    await expect(primary).toHaveAttribute("href", "/app/");
    await expect(page.locator("main .app-primary")).toHaveCount(1);

    // The facts strip: four cells that differ from app to app, from the manifest only.
    const a = messages.en.apps;
    await expect(page.locator(".app-facts dt")).toHaveText([
      a.meta.maturity,
      a.meta.version,
      s.worksWith,
      s.category,
    ]);
    await expect(page.locator(".app-facts dd")).toHaveText([
      "Stable",
      "0.2.1",
      "Contacts, Sales",
      "Finance",
    ]);

    // The sections, in order; a built-in app has "Included" where a marketplace app has "Get".
    await expect(page.locator("main > .wrap > section > h2")).toHaveText([
      a.h.purpose,
      s.canDo,
      s.keeps,
      s.safe,
      s.worksWith,
      s.info,
      s.includedTitle,
      s.developers,
      a.h.related,
    ]);
    // What it is for: the first sentence large, the rest small and quiet.
    await expect(page.locator(".app-purpose")).toHaveText(
      "Prepare customer invoices as drafts and post them to give each one its gapless legal number.",
    );
    await expect(page.locator(".app-purpose-rest")).toContainText("Posting is the one step");
    // What you can do: one card per workflow action, with the approval mark where it is needed.
    const post = page.locator(".app-card", { hasText: "Post a draft invoice" });
    await expect(post.locator("h3")).toHaveText("Post");
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
    await expect(page.locator(".app-info dt", { hasText: s.source })).toBeVisible();
    await expect(
      page.locator(".app-info a", { hasText: "crates/modules/invoicing" }),
    ).toHaveAttribute("href", /\/tree\/main\/crates\/modules\/invoicing$/);
    await expect(page.locator(".app-getbox")).toHaveText(s.included);

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
    // One plain sentence first; the command line is folded away, and still in the page.
    await expect(page.locator(".app-getbox > p").first()).toHaveText(s.getLead);
    const cmd = page.locator(".app-getbox details.app-cmd");
    await expect(cmd).toHaveJSProperty("open", false);
    await expect(cmd.locator("summary")).toHaveText(s.commandLine);
    await expect(page.locator(".app-getbox pre code")).toHaveText(
      "obop modules install-from helpdesk",
    );
    await cmd.locator("summary").click();
    await expect(page.locator(".app-getbox pre code")).toBeVisible();
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
    await expect(page.locator(".app-card h3").first()).toHaveText("Übernehmen");
    await expect(page.locator(".app-card [lang]")).toHaveCount(0);
    await expect(page.locator("h2#for-developers")).toHaveText(s.developers);
    await expect(page.locator("details#developers > summary")).toHaveText(s.developersShow);
    // Field descriptions in the technical tables are English and say so.
    await expect(page.locator("details#developers td span[lang=en]").first()).toBeAttached();
    await page.goto(`${base()}/de/apps/crm/`);
    await expect(page.locator(".app-hero .app-primary")).toHaveText(s.open);
    await expect(page.locator(".app-hero .app-primary")).toHaveAttribute("href", "/app/");
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
      // Facts: maturity, version, who it works with, category.
      const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);
      const dd = await page.locator(".app-facts dd").allTextContents();
      expect(dd[0]).toBe(cap(maturity));
      expect(dd[1]).toBe(version);
      expect(dd[2]).toBe(
        deps.length ? deps.map((d) => names[d] ?? cap(d)).join(", ") : s.standalone,
      );
      // What you can do: one card per action, the approval mark exactly where the manifest says,
      // and the cards beyond the sixth folded with a label that counts and names them.
      const can = page.locator("section", { has: page.locator("h2#can-do") });
      await expect(can.locator(".app-card")).toHaveCount(actionBlocks.length);
      await expect(can.locator(".app-card-note")).toHaveCount(approvals);
      const foldedActions = Math.max(0, actionBlocks.length - 6);
      if (foldedActions)
        await expect(can.locator(".app-more > summary")).toHaveText(
          s.showMoreActions.replace("{n}", String(foldedActions)),
        );
      else await expect(can.locator(".app-more")).toHaveCount(0);
      // What it keeps: one card per model, the same way.
      const keeps = page.locator("section", { has: page.locator("h2#keeps") });
      await expect(keeps.locator(".app-card")).toHaveCount(models);
      const foldedModels = Math.max(0, models - 6);
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
    await page.locator(".app-getbox details.app-cmd summary").click();
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
    await page.goto(`${base()}/apps/invoicing/#models`);
    const dev = page.locator("details#developers");
    await page.evaluate(() => {
      (document.querySelector("details#developers") as HTMLDetailsElement).open = false;
      window.scrollTo(0, 0);
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
    // A script that is served but wires nothing (an aborted request would also log a console error).
    await page.route("**/site/site.js", (route) =>
      route.fulfill({ contentType: "text/javascript", body: "" }),
    );
    await page.goto(`${base()}/apps/helpdesk/`);
    await page.locator(".app-getbox details.app-cmd summary").click();
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
    expect(app).toContain("obop modules install-from helpdesk");
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
          return out;
        }, messages[locale].htmlLang);
        expect(problems, `${locale} ${url}`).toEqual([]);
        checked++;
      }
    }
    expect(checked).toBe(7 * 29);
  });

  test("the apps pages show a focus ring on every stop of a Tab walk", async ({ page }) => {
    for (const [path, atLeast] of [
      ["/apps/", 28],
      ["/apps/helpdesk/", 8],
      ["/apps/accounting/", 12],
    ] as const) {
      await page.goto(`${base()}${path}`);
      let stops = 0;
      for (let i = 0; i < 100; i++) {
        await page.keyboard.press("Tab");
        const stop = await page.evaluate(() => {
          const el = document.activeElement as HTMLElement | null;
          if (!el || !el.closest("main")) return null;
          const cs = getComputedStyle(el);
          return {
            what: `${el.tagName} ${(el.textContent ?? "").trim().slice(0, 30)}`,
            style: cs.outlineStyle,
            width: parseFloat(cs.outlineWidth),
          };
        });
        if (!stop) continue;
        stops++;
        expect(stop.style, `${path}: ${stop.what}`).not.toBe("none");
        expect(stop.width, `${path}: ${stop.what}`).toBeGreaterThanOrEqual(2);
      }
      expect(stops, `${path} stops in main`).toBeGreaterThanOrEqual(atLeast);
    }
  });

  test("the apps pages keep their text at 4.5:1 or better, in both colour schemes and when hovered", async ({
    page,
  }) => {
    // Colours are read back through a canvas, so tokens written as oklch() or color-mix() count too.
    const measure = (selector: string, hover: boolean) =>
      page.evaluate(
        ([sel, hover]) => {
          const el = document.querySelector(sel as string) as HTMLElement | null;
          if (!el) return null;
          const c = document.createElement("canvas");
          c.width = c.height = 1;
          const ctx = c.getContext("2d", { willReadFrequently: true })!;
          const rgba = (color: string) => {
            ctx.clearRect(0, 0, 1, 1);
            ctx.fillStyle = "#000";
            ctx.fillStyle = color;
            ctx.fillRect(0, 0, 1, 1);
            return ctx.getImageData(0, 0, 1, 1).data;
          };
          const lum = (d: Uint8ClampedArray) => {
            const f = (v: number) => {
              const x = v / 255;
              return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
            };
            return 0.2126 * f(d[0]!) + 0.7152 * f(d[1]!) + 0.0722 * f(d[2]!);
          };
          let bg: Uint8ClampedArray = rgba("#ffffff");
          for (let n: HTMLElement | null = el; n; n = n.parentElement) {
            const d = rgba(getComputedStyle(n).backgroundColor);
            if (d[3] === 255) {
              bg = d;
              break;
            }
          }
          const fg = rgba(getComputedStyle(el).color);
          const [a, b] = [lum(fg), lum(bg)];
          void hover;
          return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
        },
        [selector, hover] as const,
      );
    const pages: [string, string[]][] = [
      [
        "/apps/helpdesk/",
        [
          ".app-primary",
          ".app-back a",
          ".app-tagline",
          ".app-facts dt",
          ".app-facts dd",
          ".app-purpose",
          ".app-purpose-rest",
          ".app-card-kicker",
          ".app-card-text",
          ".app-card-note",
          ".app-info dt",
          ".app-cmd > summary",
          ".app-dev > summary",
          ".app-getbox .fine",
        ],
      ],
      ["/apps/accounting/", [".app-more > summary"]],
      ["/apps/", [".store-tag", ".store-name", ".store-lead", ".store-group h3"]],
      ["/de/apps/helpdesk/", [".app-note"]],
    ];
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      for (const [path, selectors] of pages) {
        await page.goto(`${base()}${path}`);
        for (const sel of selectors) {
          const rest = await measure(sel, false);
          expect(rest, `${scheme} ${path} ${sel} is on the page`).not.toBeNull();
          expect(rest!, `${scheme} ${path} ${sel}`).toBeGreaterThanOrEqual(4.5);
          // Hovered: the two folds and the primary pill change their colours.
          if ([".app-more > summary", ".app-dev > summary", ".app-primary"].includes(sel)) {
            await page.locator(sel).first().hover();
            await page.waitForTimeout(250);
            const hovered = await measure(sel, true);
            expect(hovered!, `${scheme} ${path} ${sel} hovered`).toBeGreaterThanOrEqual(4.5);
            await page.mouse.move(0, 0);
          }
        }
      }
    }
  });

  test("the pill buttons keep an edge in forced colours, and a printed page keeps the technical part", async ({
    page,
  }) => {
    await page.emulateMedia({ forcedColors: "active" });
    await page.goto(`${base()}/apps/accounting/`);
    for (const sel of [".app-primary", ".app-more > summary", ".app-dev > summary"]) {
      const border = await page
        .locator(sel)
        .first()
        .evaluate((el) => {
          const cs = getComputedStyle(el);
          return { style: cs.borderTopStyle, width: parseFloat(cs.borderTopWidth) };
        });
      expect(border, sel).toEqual({ style: "solid", width: 1 });
    }
    await page.emulateMedia({ forcedColors: "none", media: "print" });
    await expect(page.locator(".app-primary")).toBeHidden();
    await expect(page.locator("details#developers")).toHaveJSProperty("open", false);
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeVisible();
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

  test("pages are usable with the keyboard: skip link first, then the header", async ({ page }) => {
    await page.goto(`${base()}/docs/`);
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: "Skip to the content" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#main$/);
  });
});

// The public website (web/apps/site, served at the root of the control plane): static HTML that
// must read and navigate without JavaScript, be well-formed for crawlers and agents, and be
// localised. Runs against the built web/site directory.
import { messages } from "../../../apps/site/i18n/index.ts";
import { platformUrl } from "../support/env.ts";
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
      page.getByText("Planned prices, billed yearly, excluding VAT. Not yet charged."),
    ).toBeVisible();
    await expect(page.locator("#faq details")).toHaveCount(8);
    await page.locator("#faq summary").first().click();
    await expect(page.locator("#faq details").first()).toHaveJSProperty("open", true);
  });

  test("the apps index filters by status and category with CSS only", async ({ page }) => {
    await page.goto(`${base()}/apps/`);
    await expect(page.locator(".apps .app")).toHaveCount(28);
    await page.locator('label[for="st-live"]').click();
    await expect(page.locator(".apps .app:visible")).toHaveCount(6);
    await page.locator('label[for="st-all"]').click();
    await page.locator('label[for="cat-finance"]').click();
    const names = await page.locator(".apps .app:visible h3").allTextContents();
    expect(names.sort()).toEqual(["Accounting", "Expenses", "Invoicing"]);
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

  test("an app page lists models, fields, workflows, approvals and agent limits", async ({
    page,
  }) => {
    await page.goto(`${base()}/apps/invoicing/`);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Invoicing");
    await expect(page.getByRole("heading", { level: 2, name: "Models" })).toBeVisible();
    await expect(page.getByRole("cell", { name: "total_minor" })).toBeVisible();
    await expect(page.getByRole("cell", { name: "invoice.post" })).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "What agents can and cannot do" }),
    ).toBeVisible();
    await expect(page.getByText("It can only propose these steps")).toContainText("invoice.post");

    await page.goto(`${base()}/apps/helpdesk/`);
    await expect(
      page.locator("pre code").filter({ hasText: "obop modules install-from helpdesk" }),
    ).toBeVisible();
    await expect(page.getByRole("cell", { name: "requester_email" })).toBeVisible();
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

  test("the pricing page labels every price as planned", async ({ page }) => {
    await page.goto(`${base()}/pricing/`);
    await expect(page.getByText("Planned, not yet charged")).toBeVisible();
    for (const price of ["€0", "€49", "€249", "€899"])
      await expect(page.getByText(price).first()).toBeVisible();
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
    "/docs/",
    "/docs/agents/mcp/",
    "/pricing/",
    "/search/",
    "/404.html",
    "/de/",
    "/fi/apps/",
    "/marketplace/",
  ];

  test("sample pages: named controls, heading order, landmarks, unique ids, image alt, language", async ({
    page,
  }) => {
    for (const path of samples) {
      await page.goto(`${base()}${path}`);
      expect(await unnamedControls(page), `${path} controls`).toEqual([]);
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

  test("no horizontal scroll at 390px and 320px", async ({ page }) => {
    for (const w of [390, 320]) {
      await page.setViewportSize({ width: w, height: 800 });
      for (const path of ["/", "/apps/", "/docs/agents/mcp/", "/apps/helpdesk/"]) {
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

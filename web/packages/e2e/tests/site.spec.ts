// The public website (web/apps/site, served at the root of the control plane): static HTML that
// must read and navigate without JavaScript, be well-formed for crawlers and agents, and be
// localised. Runs against the built web/site directory.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { messages } from "../../../apps/site/i18n/index.ts";
import { CLAIMS } from "../../../apps/site/scripts/lib/claims.ts";
import { TECHNICAL, technicalIn } from "../../../apps/site/scripts/lib/plain.ts";
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
  // A longer wait than the default 10 s: one run in three stalled on the body of a sitemap (headers 200, gzip,
  // chunked) and passed on a rerun; the cause is not known.
  const xml = await (
    await request.get(`${base()}/sitemap-${locale}.xml`, { timeout: 30_000 })
  ).text();
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

/**
 * Waits for the web fonts the page has asked for, so that a sweep measures the words in the font the
 * visitor gets and not in the fallback of the moment before it loaded (a face for the Polish letters
 * was still loading right after domcontentloaded).
 */
const settle = (page: import("@playwright/test").Page) =>
  page.evaluate(() => document.fonts.ready.then(() => undefined));

/**
 * The words of the elements that match `selector` which the browser has drawn on two lines, as the
 * visitor's page is drawn: nothing is switched off, so the test sees what the safety nets
 * (overflow-wrap, hyphens) really do. A word is a run of letters and digits up to a hyphen
 * ("Apache-2.0" may break after its hyphen). `wide` counts the words that are wider than the box
 * holding them: nothing can keep those whole, so with `allowWide` they may break (and are only
 * counted, and listed in `wideWords`); every other word that is broken is returned in `cut`. Text
 * inside an element that matches `skip` is not looked at. With `parts`, an underscore and a dot
 * end a word too: a code token (`public_directory`, `crm.user`) may break after them, and each
 * part of it must stay whole.
 */
async function brokenWords(
  page: import("@playwright/test").Page,
  selector: string,
  allowWide = false,
  skip = "",
  parts = false,
) {
  await settle(page);
  return page.evaluate(
    ({ sel, allowWide, skip, parts }) => {
      const cut: string[] = [];
      const wideWords: string[] = [];
      let wide = 0;
      for (const el of document.querySelectorAll(sel)) {
        const own = getComputedStyle(el);
        const room =
          (el as HTMLElement).clientWidth -
          parseFloat(own.paddingInlineStart) -
          parseFloat(own.paddingInlineEnd);
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          if (skip && n.parentElement?.closest(skip)) continue;
          const re = parts ? /[^\s­_.-]+[_.-]?/g : /[^\s­-]+-?/g;
          for (let m = re.exec(n.textContent ?? ""); m; m = re.exec(n.textContent ?? "")) {
            const r = document.createRange();
            r.setStart(n, m.index);
            r.setEnd(n, m.index + m[0].length);
            let rects = [...r.getClientRects()].filter((x) => x.width > 0);
            // A word that follows a soft hyphen at which the line broke: the browser reports the hyphen
            // it drew at the end of the line above as the first rectangle of the word. That hyphen is not
            // part of the word, which stands whole on the next line. It is told by its width (the width
            // of a hyphen in the same font) and by lying on the line above.
            if (n.textContent?.[m.index - 1] === "­" && rects.length > 1) {
              const hy = document.createElement("span");
              hy.style.cssText = "position:absolute;visibility:hidden;white-space:nowrap";
              hy.textContent = "-";
              n.parentElement!.appendChild(hy);
              const hyphen = hy.getBoundingClientRect().width;
              hy.remove();
              if (Math.abs(rects[0]!.width - hyphen) < 1.5 && rects[0]!.top < rects[1]!.top - 3)
                rects = rects.slice(1);
            }
            const tops = rects.map((x) => x.top);
            if (tops.length < 2 || Math.max(...tops) - Math.min(...tops) <= 3) continue;
            // Two lines: is the word wider than the box it sits in?
            const probe = document.createElement("span");
            probe.style.cssText =
              "position:absolute;visibility:hidden;white-space:nowrap;hyphens:manual;overflow-wrap:normal;word-break:normal";
            probe.textContent = m[0];
            n.parentElement!.appendChild(probe);
            const natural = probe.getBoundingClientRect().width;
            probe.remove();
            // clientWidth is rounded to a whole pixel: a word within half a pixel of the box is as good as wider.
            if (allowWide && natural > room - 0.5) {
              wide++;
              wideWords.push(m[0]);
            } else cut.push(`${m[0]} (${Math.round(natural)} px in ${Math.round(room)} px)`);
          }
        }
      }
      return { cut, wide, wideWords };
    },
    { sel: selector, allowWide, skip, parts },
  );
}

/**
 * A page of the site in a browser whose own text-size setting is `percent` of the default (what a
 * visitor who has chosen larger text gets: rem units and rem media queries both scale, which a
 * style rule on the root element does not do). The caller closes the context.
 */
async function pageWithTextSize(
  browser: import("@playwright/test").Browser,
  percent: number,
  viewport = { width: 320, height: 900 },
) {
  const context = await browser.newContext({ viewport, colorScheme: "light", locale: "en-US" });
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  await cdp.send("Page.setFontSizes", {
    fontSizes: { standard: Math.round((16 * percent) / 100), fixed: 13 },
  });
  return { page, context };
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
      "Prepare invoices for your customers as drafts, then post each one: posting gives it its own number, in a yearly series with no gaps.",
    );
    await expect(page.locator(".app-purpose-rest")).toContainText(
      "Posting needs a person with the right permission",
    );
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
    await expect(dev.locator("summary")).toHaveText(s.developersShow, { useInnerText: true });
    // Both labels are in the markup, with a space between them, so a reader that ignores the style
    // sheet (a crawler, an agent, a reader mode) does not get "detailsHide".
    expect(await dev.locator("summary").evaluate((el) => el.textContent)).toBe(
      `${s.developersShow} ${s.developersHide}`,
    );
    const html = await (await page.request.get(`${base()}/apps/invoicing/`)).text();
    expect(html).toContain(
      `>${s.developersShow}</span> <span class="app-hide">${s.developersHide}</span>`,
    );
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeHidden();
    await dev.locator("summary").click();
    await expect(dev).toHaveJSProperty("open", true);
    // The button says what it does now: Hide while the part is open.
    await expect(dev.locator("summary")).toHaveText(s.developersHide, { useInnerText: true });
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
    // The one card that has plain words of its own in data/apps.ts (Helpdesk's "Response times") is
    // English and says so, in its heading and in its text; every other card is German.
    await expect(page.locator(".app-card [lang]")).toHaveCount(2);
    await expect(page.locator(".app-card", { has: page.locator("[lang=en]") })).toHaveCount(1);
    await expect(page.locator(".app-card", { has: page.locator("[lang=en]") })).toContainText(
      "Response times",
    );
    await expect(page.locator("h2#for-developers")).toHaveText(s.developers);
    await expect(page.locator("details#developers > summary")).toHaveText(s.developersShow, {
      useInnerText: true,
    });
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
      // The kinds of detail the manifest withholds: its data classes, in the order of the table.
      const withheldClasses = new Set(
        fieldBlocks.flatMap((b) => {
          const c = withheldClass.exec(b)?.[1];
          return c ? [c] : [];
        }),
      );
      const withheld = withheldClasses.size > 0;
      // A money field that an AI agent reads in clear (its class is none and it is not personal data):
      // only then does the withheld line add that a price or a total is shown. Neither of these two
      // manifests has one (every amount in Accounting is financial, and Connectors has none).
      const moneyInClear = fieldBlocks.some(
        (b) =>
          /^\s*kind\s*=\s*"money"/m.test(b) &&
          !withheldClass.test(b) &&
          !/^\s*personal_data\s*=\s*true/m.test(b),
      );
      expect(moneyInClear, `${app.slug}: no amount in clear`).toBe(false);
      const version = /^version\s*=\s*"([^"]+)"/m.exec(toml)![1]!;
      // The word is the file's own, for a built-in app too (its snapshot says stable, as the Rust does).
      const maturity = /^maturity\s*=\s*"([^"]+)"/m.exec(toml)![1]!;
      if (app.builtin) expect(maturity, "the Connectors snapshot says stable").toBe("stable");
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
      // Every marked card says it in a few plain words next to the icon: an icon alone tells a
      // reader nothing. (The last span of a note holds the words; the first holds the icon.) The
      // words are drawn, not left to screen readers: each one has a box, in whichever fold it sits.
      const words = can.locator(".app-card-note > span:last-child");
      await expect(words).toHaveText(Array(approvals).fill(s.needsApproval));
      await can
        .locator(".app-more")
        .evaluateAll((all) => all.forEach((d) => ((d as HTMLDetailsElement).open = true)));
      for (const w of await words.all()) {
        const box = await w.boundingBox();
        expect(box, "the approval words of a card are drawn").not.toBeNull();
        expect(box!.width, "the approval words of a card are drawn").toBeGreaterThan(40);
      }
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
      // AI agents and this app: the sentence that says what an AI agent is, then exactly the lines the
      // manifest earns.
      const kinds = (["financial", "confidential_hr", "special_category", "secret"] as const)
        .filter((c) => withheldClasses.has(c))
        .map((c) => s.safeKinds[c]);
      const lines = [
        ...(masked ? [s.safeMasked] : []),
        ...(withheld
          ? [
              [
                s.safeWithheld.replace(
                  "{list}",
                  new Intl.ListFormat("en", { style: "long", type: "conjunction" }).format(kinds),
                ),
                ...(moneyInClear ? [s.safeMoneyClear] : []),
              ].join(" "),
            ]
          : []),
        approvals ? s.safeApprove : s.safeApproveNone,
      ];
      await expect(page.locator(".app-safe li")).toHaveText(lines);
      await expect(page.locator("#safe-with-agents")).toHaveText(s.safe);
      await expect(page.locator("#safe-with-agents + .app-safe-what + .app-safe")).toHaveCount(1);
      await expect(page.locator(".app-safe-what")).toHaveText(s.safeWhat);
      // Said out loud for the two pages: what each withholds, and nothing it does not.
      if (app.slug === "accounting")
        expect(lines).toContain(
          "AI agents never see the fields marked as sensitive. In this app: sensitive financial details.",
        );
      if (app.slug === "connectors")
        expect(lines).toContain(
          "AI agents never see the fields marked as sensitive. In this app: secrets (such as sign-in details).",
        );
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
    // The large lead is one short sentence, in plain words (the colon cut is a unit test).
    await page.goto(`${base()}/apps/hr/`);
    await expect(page.locator(".app-purpose")).toHaveText(
      "A register of employees, departments and leave requests, with strict rules about who sees what.",
    );
    await expect(page.locator(".app-purpose-rest")).toContainText(
      "Pay, bank details and national ID number",
    );
    await expect(page.locator(".app-read")).toHaveCount(0);
    // A rest that is a wall of text sits under "Read more", closed, and is still in the page.
    await page.goto(`${base()}/apps/connectors/`);
    const read = page.locator("details.app-read");
    await expect(read).toHaveJSProperty("open", false);
    await expect(read.locator("summary")).toHaveText(s.readMore);
    // The first sentence of the rest is the caveat of the page and is always shown; the others fold.
    await expect(page.locator(".app-purpose ~ .app-purpose-rest")).toHaveText(
      "These are technical controls that support GDPR compliance, not legal advice or a certification.",
    );
    await expect(page.locator(".app-purpose ~ .app-purpose-rest")).toBeVisible();
    await expect(read.locator(".app-purpose-rest")).toBeHidden();
    expect(await page.locator(".app-purpose").innerText()).toBe(
      "Connect your other tools to obop, with privacy controls built in.",
    );
    await read.locator("summary").click();
    await expect(read.locator(".app-purpose-rest")).toContainText("Article 30");
    // The Markdown copy says the whole purpose as one paragraph, as the page does.
    const md = await (await page.request.get(`${base()}/apps/hr.md`)).text();
    expect(md).toContain(
      "who sees what. Pay, bank details and national ID number are visible to HR managers only",
    );
  });

  test("the first sentence is plain words, and what is technical sits under For developers", async ({
    page,
    request,
  }) => {
    // The large first sentence of every page is for a person who does not build software: none of
    // the words of apps/site/scripts/lib/plain.ts is in it (nor in the quiet paragraph under it, nor
    // in the cards that are shown, nor in the lines about AI agents). What was left out is the
    // first paragraph of the technical part, in the page and in the Markdown copy. The list is the
    // one the unit tests read, so the two cannot differ.
    // The rule can fail: a text with such words in it is found.
    expect(technicalIn("a hash and a payload")).not.toBeNull();
    expect(technicalIn("Keep track of possible sales.")).toBeNull();
    // Every word the rule lists is found by its own example (the unit test pins the list itself).
    expect(TECHNICAL.length).toBeGreaterThanOrEqual(23);
    for (const { word, example } of TECHNICAL)
      expect(technicalIn(example), `the rule must find "${word}" in "${example}"`).not.toBeNull();
    for (const slug of [
      "dashboards",
      "sign",
      "connectors",
      "invoicing",
      "products",
      "contacts",
      "ecommerce",
      "accounting",
      "pos",
      "email-marketing",
    ]) {
      await page.goto(`${base()}/apps/${slug}/`);
      const lead = await page.locator(".app-purpose").innerText();
      expect(technicalIn(lead), `${slug} lead: ${lead}`).toBeNull();
      expect(lead.split(/\s+/).length, `${slug} lead length`).toBeLessThanOrEqual(25);
      // One sentence: the caveat of Email Marketing ("obop does not send the emails itself") was once
      // a second sentence in the large type, because the splitter ends a sentence before a capital.
      expect(lead.match(/[.!?](?=\s|$)/g), `${slug} lead is one sentence: ${lead}`).toHaveLength(1);
      // Open the fold, where there is one: the rest is what sits in it.
      const fold = page.locator("details.app-read > summary");
      if (await fold.count()) await fold.click();
      const rest = (await page.locator(".app-purpose-rest").allInnerTexts()).join(" ");
      expect(technicalIn(rest), `${slug} rest: ${rest}`).toBeNull();
      // The cards that are shown, and the sentence and the lines about AI agents.
      const cards = [
        ...(await page.locator("#can-do ~ .app-cards > .app-card").allInnerTexts()),
        ...(await page.locator("#keeps ~ .app-cards > .app-card").allInnerTexts()),
      ];
      for (const card of cards)
        expect(technicalIn(card.replace(/\s+/g, " ")), `${slug} card: ${card}`).toBeNull();
      for (const line of await page.locator(".app-safe li").allInnerTexts())
        expect(technicalIn(line), `${slug} safe line: ${line}`).toBeNull();
      const what = await page.locator(".app-safe-what").innerText();
      expect(technicalIn(what), `${slug} safe sentence: ${what}`).toBeNull();
    }
    // The page the review named first: the dashboards sentence says what a person gets.
    await page.goto(`${base()}/apps/dashboards/`);
    await expect(page.locator(".app-purpose")).toHaveText(
      "Build dashboards that show your numbers as charts and tables.",
    );
    // The technical sentence is in the fold, closed until it is opened, and in the Markdown copy
    // under "For developers" only.
    const dev = page.locator("details#developers");
    await expect(dev).toContainText("The package never runs a query");
    await expect(dev.getByText("The package never runs a query")).toBeHidden();
    await dev.locator("summary").click();
    await expect(dev.getByText("The package never runs a query")).toBeVisible();
    const md = await (await request.get(`${base()}/apps/dashboards.md`)).text();
    const at = md.indexOf(`## ${messages.en.apps.store.developers}`);
    expect(md.slice(at)).toContain("The package never runs a query");
    expect(md.slice(0, at)).not.toContain("The package never runs a query");
  });

  test("an experimental app says in one plain line that it is still being tested", async ({
    page,
    request,
  }) => {
    const s = messages.en.apps.store;
    // The five apps whose manifest says "experimental" have the line under the facts strip, drawn
    // and in reading order right after it; the stable and beta ones do not.
    const experimental = ["manufacturing", "field-service", "pos", "sign", "dashboards"];
    for (const slug of [...experimental, "invoicing", "helpdesk", "crm", "accounting"]) {
      await page.goto(`${base()}/apps/${slug}/`);
      const maturity = await page.locator(".app-facts dd").first().innerText();
      const note = page.locator(".app-experimental");
      if (experimental.includes(slug)) {
        expect(maturity, slug).toBe(messages.en.apps.meta.maturities.experimental);
        await expect(note, slug).toHaveText(s.experimentalNote);
        await expect(note, `${slug}: the line is drawn`).toBeVisible();
        const order = await page.evaluate(() => {
          const strip = document.querySelector(".app-facts-box")!;
          const line = document.querySelector(".app-experimental")!;
          return strip.nextElementSibling === line;
        });
        expect(order, `${slug}: the line follows the strip`).toBe(true);
        const md = await (await request.get(`${base()}/apps/${slug}.md`)).text();
        expect(md, slug).toContain(s.experimentalNote);
      } else {
        expect(maturity, slug).not.toBe(messages.en.apps.meta.maturities.experimental);
        await expect(note, slug).toHaveCount(0);
      }
    }
    // In every language, from the language's own words.
    for (const locale of Object.keys(messages).filter((l) => l !== "en")) {
      await page.goto(`${base()}/${locale}/apps/manufacturing/`);
      await expect(page.locator(".app-experimental")).toHaveText(
        (messages as Record<string, typeof messages.en>)[locale]!.apps.store.experimentalNote,
      );
    }
  });

  test("the six built-in apps say Stable on every surface, in every language, and none of them says Beta", async ({
    page,
    request,
  }) => {
    // The owner's ruling of 2026-11-10: the built-in apps show the maturity their snapshots and the Rust
    // code say (stable), not the word the loader once forced. The pages as built: the strip, the
    // information list, the text of the page (the folded technical part included), the structured
    // data, the descriptions, the Markdown copy, the index and llms-full.txt. (The footer of every page,
    // the home page pill and FAQ and llms.txt now say that obop is below version 1.0; they are outside
    // the main content of these pages, which is what this test reads.)
    const slugs = ["contacts", "products", "crm", "sales", "invoicing", "connectors"];
    for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
      const m = messages[locale].apps;
      const prefix = locale === "en" ? "" : `/${locale}`;
      const beta = new RegExp(`(?<!\\p{L})${m.meta.maturities.beta}(?!\\p{L})`, "iu");
      // The rule can fail: the language's own word for beta is found, and "Betalning" is not it.
      expect(`Maturity: ${m.meta.maturities.beta}`).toMatch(beta);
      expect(`Maturity: ${m.meta.maturities.stable}`).not.toMatch(beta);
      for (const slug of slugs) {
        const where = `${locale} ${slug}`;
        await page.goto(`${base()}${prefix}/apps/${slug}/`, { waitUntil: "domcontentloaded" });
        await expect(page.locator(".app-facts dd").first(), where).toHaveText(
          m.meta.maturities.stable,
        );
        await expect(
          page.locator(".app-info > div", { hasText: m.meta.maturity }).locator("dd").first(),
          where,
        ).toHaveText(m.meta.maturities.stable);
        const text = await page.locator("main").evaluate((el) => el.textContent ?? "");
        expect(text, `${where}: the page text`).not.toMatch(beta);
        const data = await page
          .locator(
            'script[type="application/ld+json"], meta[name="description"], meta[property="og:description"]',
          )
          .evaluateAll((els) =>
            els.map((el) => (el.tagName === "META" ? el.getAttribute("content") : el.textContent)),
          );
        expect(data.join(" "), `${where}: structured data and descriptions`).not.toMatch(beta);
        const md = await (await request.get(`${base()}${prefix}/apps/${slug}.md`)).text();
        expect(md, `${where}: the Markdown copy`).not.toMatch(beta);
        expect(md, where).toContain(`**${m.meta.maturity}:** ${m.meta.maturities.stable}`);
      }
      // The index: the tiles and rows name no maturity, and its lead no longer says "still in beta".
      await page.goto(`${base()}${prefix}/apps/`, { waitUntil: "domcontentloaded" });
      expect(await page.locator("main").evaluate((el) => el.textContent ?? ""), locale).not.toMatch(
        beta,
      );
      expect(
        await (await request.get(`${base()}${prefix}/apps.md`)).text(),
        `${locale} apps.md`,
      ).not.toMatch(beta);
    }
    // llms-full.txt: the six English sections say Stable once in the strip, once in the information
    // list and once in the technical table.
    const sections = (await (await request.get(`${base()}/llms-full.txt`)).text())
      .split("\n\n---\n\n")
      .filter((section) => section.includes("\n## What it is for\n"));
    const builtin = sections.filter((section) =>
      slugs.some((slug) => section.trimEnd().endsWith(`/apps/${slug}/`)),
    );
    expect(builtin, "the six built-in sections of llms-full.txt").toHaveLength(6);
    for (const section of builtin) {
      expect(section.split("**Maturity:** Stable").length - 1).toBe(3);
      expect(section).not.toMatch(/(?<!\p{L})beta(?!\p{L})/iu);
    }
    // The marketplace apps keep their manifests' words: some are beta and some experimental.
    const maturities = new Set(
      sections.flatMap((section) =>
        [...section.matchAll(/\*\*Maturity:\*\* (\w+)/g)].map((x) => x[1]),
      ),
    );
    expect([...maturities].sort()).toEqual(["Beta", "Experimental", "Stable"]);
  });

  // The owner's ruling of 2026-11-10: the app pages say "AI agent", not "AI assistant", and each page
  // explains the word once. The word each language uses on its home page and in its docs; "asistente"
  // alone is also an attendee in the product's own Spanish labels of an appointment.
  const agent = {
    en: /\bAI agents?\b/,
    sv: /\bAI-agent(?:er)?\b/,
    de: /\bKI-Agent(?:en)?\b/,
    fr: /\bagents? IA\b/,
    pl: /\bagen(?:t|ci) AI\b/i,
    fi: /\btekoälyagentti|\btekoälyagentit/i,
    es: /\bagentes? de IA\b/,
  } as const;
  const assistant = /assistants?\b|assistent|asystent|asystenc|avustaj|asistentes? de IA/i;

  test("the sentence that says what an AI agent is is the owner's, and the rule that finds 'assistant' can fail", () => {
    expect(messages.en.apps.store.safeWhat).toBe(
      "An AI agent is an AI program that works in obop for a person, and can do no more than that person may.",
    );
    for (const example of [
      "AI assistants and this app",
      "KI-Assistenten und diese App",
      "Les assistants IA",
      "Asystenci AI i ta aplikacja",
      "Tekoälyavustajat näkevät",
      "Los asistentes de IA",
      "AI-assistenter och den här appen",
    ])
      expect(example).toMatch(assistant);
    expect("Nombre del asistente").not.toMatch(assistant);
  });

  // One test for each language (28 pages each), so that none runs past the 30 seconds a test has.
  for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
    test(`${locale}: every app page explains what an AI agent is once, right under the heading, in the language's own word, and says assistant nowhere`, async ({
      page,
      request,
    }) => {
      const s = messages[locale].apps.store;
      const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u));
      expect(urls, locale).toHaveLength(28);
      for (const url of urls) {
        const where = `${locale} ${url}`;
        await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
        await expect(page.locator("#safe-with-agents"), where).toHaveText(s.safe);
        expect(s.safe, where).toMatch(agent[locale]);
        await expect(
          page.locator("#safe-with-agents + .app-safe-what + .app-safe"),
          where,
        ).toHaveCount(1);
        await expect(page.locator(".app-safe-what"), where).toHaveText(s.safeWhat);
        // The plain part is the page without its folded technical part; the sentence is in it once,
        // and nowhere else on the page.
        const plain = await page.locator("main").evaluate((el) => {
          const copy = el.cloneNode(true) as HTMLElement;
          copy.querySelector("#developers")?.remove();
          return copy.textContent ?? "";
        });
        expect(plain.split(s.safeWhat).length - 1, `${where}: the sentence, once`).toBe(1);
        expect(plain, `${where}: no assistant`).not.toMatch(assistant);
        const all = await page.locator("main").evaluate((el) => el.textContent ?? "");
        expect(all.split(s.safeWhat).length - 1, `${where}: not in the technical part`).toBe(1);
      }
    });
  }

  test("the section after the technical part lists the other apps of the category, and says so", async ({
    page,
  }) => {
    // It used to be called "Related apps", which promises more than it holds: the apps of the same
    // category, nothing else.
    for (const [locale, slug] of [
      ["en", "manufacturing"],
      ["de", "fleet"],
      ["fi", "subscriptions"],
    ] as const) {
      const m = messages[locale].apps;
      await page.goto(`${base()}${locale === "en" ? "" : `/${locale}`}/apps/${slug}/`);
      const heading = page.locator("main > .wrap > section:last-child > h2");
      await expect(heading).toHaveText(m.h.related);
      expect(m.h.related, locale).not.toMatch(
        /related|verwandt|liittyv|relacionad|liée|powiąz|relaterad/i,
      );
      await expect(
        page.locator("main > .wrap > section:last-child .app-icons a").first(),
      ).toBeVisible();
    }
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
    // The ring that the link puts on the heading hugs its text: the heading has no padding of its
    // own (the rule above it is drawn outside its box), so the ring is one line high, not 60 px.
    await expect(page.locator("#models")).toBeFocused();
    // The rule above the heading is drawn: a declared border style is not enough (a rule with no box,
    // no content or no width has one and draws nothing), so the rule's own box is read (the used
    // size of the ::before) and so is the air above the heading that it sits in.
    const ring = await page.locator("#models").evaluate((el) => {
      const before = getComputedStyle(el, "::before");
      const heading = el.getBoundingClientRect();
      const above = el.previousElementSibling!.getBoundingClientRect();
      return {
        height: heading.height,
        line: parseFloat(getComputedStyle(el).lineHeight),
        padding: getComputedStyle(el).paddingBlockStart,
        rule: {
          style: before.borderBlockStartStyle,
          border: parseFloat(before.borderBlockStartWidth),
          display: before.display,
          content: before.content,
          width: parseFloat(before.width),
          offset: parseFloat(before.top),
        },
        gap: heading.top - above.bottom,
        wide: heading.width,
      };
    });
    expect(ring.padding).toBe("0px");
    expect(ring.height).toBeLessThan(ring.line * 1.5);
    expect(ring.rule.style, "the rule above the heading is solid").toBe("solid");
    expect(ring.rule.border, "the rule above the heading has a line").toBeGreaterThanOrEqual(1);
    expect(ring.rule.display, "the rule above the heading is shown").not.toBe("none");
    expect(ring.rule.content, "the rule above the heading has a box").toBe('""');
    expect(ring.rule.width, "the rule above the heading is as wide as the heading").toBeGreaterThan(
      ring.wide - 1,
    );
    expect(ring.rule.offset, "the rule is drawn above the heading, not on it").toBeLessThan(-10);
    // The rule sits in the air between this heading and what is above it, which is a section break.
    expect(ring.gap, "air above the heading").toBeGreaterThanOrEqual(48);
    expect(-ring.rule.offset, "the rule is inside that air").toBeLessThan(ring.gap);
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
    // The page breaks a table token after an underscore or a dot with <wbr>; the copy, and
    // llms-full.txt which agents read, keep the plain token in backticks and no HTML tag.
    expect(app).toContain("`requester_email`");
    expect(app).not.toMatch(/<wbr>|<code>/);
    for (const url of ["/apps/crm.md", "/apps/accounting.md", "/de/apps/hr.md", "/llms-full.txt"]) {
      const text = await (await request.get(`${base()}${url}`)).text();
      expect(text, `${url}: no <wbr> in the Markdown`).not.toContain("<wbr>");
    }
    const crm = await (await request.get(`${base()}/apps/crm.md`)).text();
    expect(crm).toContain("`contact_email`");
    expect(crm).toContain("`crm.user`");
  });

  test("the wordings on the list of false sentences are on no deployed apps page, Markdown copy or llms-full.txt", async ({
    request,
  }) => {
    // The pages as built, not as the unit tests build them from the data: the words of each sentence in
    // apps/site/scripts/lib/claims.ts were once on a page and the module does not stand behind them.
    // (The product's own translations of the module are not looked at: they say the old words, which is
    // why a card that corrects one is English.)
    const urls = (await pagesOf(request, "en")).filter((u) => /^\/apps\/.+\/$/.test(u));
    expect(urls).toHaveLength(28);
    // The list is pinned, and every row can fail: it catches the sentence it was written for and
    // leaves alone the true sentences that stand where it was (claims.ts).
    expect(CLAIMS).toHaveLength(60);
    for (const { re, example, fine, why } of CLAIMS) {
      expect(example, `${re}: ${why}`).toMatch(re);
      for (const ok of fine ?? []) expect(ok, `${re} must leave this alone`).not.toMatch(re);
    }
    const texts = (await Promise.all(
      urls.map(async (u) => [
        u,
        await (await request.get(`${base()}${u.replace(/\/$/, "")}.md`)).text(),
      ]),
    )) as [string, string][];
    // The index holds the lead of the index, and llms.txt, which agents read first, repeats it (its
    // other sentences, about agents and approval, are the site's own and are not what the list is for).
    texts.push(["/apps.md", await (await request.get(`${base()}/apps.md`)).text()]);
    const all28 = CLAIMS.find((c) => c.re.source.includes("All 28"));
    if (!all28) throw new Error("the row for 'All 28 obop apps' is missing from claims.ts");
    expect(await (await request.get(`${base()}/llms.txt`)).text(), "llms.txt").not.toMatch(
      all28.re,
    );
    // llms-full.txt, which agents read, holds every page of the site: the app pages are its sections
    // that have "What it is for" (the changelog and the docs may use the same words for other things).
    const full = (await (await request.get(`${base()}/llms-full.txt`)).text())
      .split("\n\n---\n\n")
      .filter((section) => section.includes("\n## What it is for\n"));
    expect(full.length, "app sections in llms-full.txt").toBeGreaterThanOrEqual(28);
    full.forEach((section, i) => texts.push([`/llms-full.txt (app section ${i + 1})`, section]));
    for (const [url, text] of texts)
      for (const { re, why } of CLAIMS) expect(text, `${url} ${re}: ${why}`).not.toMatch(re);
    // And the HTML of the pages that carried the worst of them says what is true.
    const html = await (await request.get(`${base()}/apps/accounting/`)).text();
    expect(html).toContain("This is a label only: entries can still be posted into it.");
    expect(await (await request.get(`${base()}/apps/purchase/`)).text()).toContain(
      "By itself, this does not contact the vendor.",
    );
  });

  test("the line about what an AI agent never sees names only what the app withholds, and the log is not said to be append-only", async ({
    request,
  }) => {
    // Money is in clear on these four pages (a vehicle's cost, an order total, a sale's amounts: their
    // data class is none), so none of them may say that money, bank details or pay are hidden.
    const line = async (slug: string) => {
      const md = await (await request.get(`${base()}/apps/${slug}.md`)).text();
      return /^- (AI agents never see the fields marked as sensitive\..*)$/m.exec(md)?.[1] ?? "";
    };
    const never = "AI agents never see the fields marked as sensitive. In this app:";
    const tail = " A price or a total is shown unless its field is one of these.";
    expect(await line("fleet")).toBe(`${never} confidential HR details.${tail}`);
    expect(await line("purchase")).toBe(`${never} sensitive financial details.${tail}`);
    expect(await line("pos")).toContain("sensitive financial details");
    expect(await line("ecommerce")).toBe(
      `${never} sensitive financial details and secrets (such as sign-in details).${tail}`,
    );
    for (const slug of ["fleet", "purchase", "pos", "ecommerce"]) {
      expect(await line(slug), slug).not.toMatch(/money|amount|bank|\bpay\b/i);
      expect(await line(slug), `${slug}: an amount is in clear`).toMatch(
        / A price or a total is shown unless its field is one of these\.$/,
      );
    }
    // Where no amount is in clear for an AI agent the sentence is not there (it was the tail of every
    // such line): Accounting, Expenses, Timesheets, Contacts, HR and Connectors.
    for (const slug of ["accounting", "expenses", "timesheets", "contacts", "hr", "connectors"]) {
      const text = await line(slug);
      expect(text, `${slug}: the line is there`).toContain(never);
      expect(text, `${slug}: no price or total is in clear`).not.toContain("A price or a total");
    }
    // An app that withholds nothing says nothing of the kind.
    expect(await line("crm")).toBe("");
    // The transfer log is deleted by the purge, and the technical part lists no delete for a model
    // that has none.
    const connectors = await (await request.get(`${base()}/apps/connectors.md`)).text();
    expect(connectors).toContain("not edited by people");
    expect(connectors).toContain("`connection.delete`");
    for (const model of ["data_flow_log", "sync_run", "consent", "egress_policy"])
      expect(connectors, model).not.toContain(`\`${model}.delete\``);
    // The contact's erase says what else it reaches.
    expect(await (await request.get(`${base()}/apps/contacts.md`)).text()).toContain(
      "The Connectors app also deletes the code names and links it kept for this contact.",
    );
    // A planning manager records the confirmation: the employee does not.
    expect(await (await request.get(`${base()}/apps/planning.md`)).text()).toContain(
      "Only a planning manager can do this.",
    );
  });

  // The caveats that only a developer needs are under "For developers" (data/apps.ts `limit` and
  // `limits`), in the page and in the Markdown copy, and not in the plain text above it. The sentences
  // are written out here, not read from the site's data, so a change of the data is a change here.
  test("the caveats that only a developer needs are in the folded part, and in English on a translated page", async ({
    request,
  }) => {
    const folded = [
      {
        slug: "ecommerce",
        text: "The refund through Stripe is tested only against a local stand-in, not against Stripe itself.",
      },
      { slug: "pos", text: "The payments are checked only when the sale is paid." },
      { slug: "pos", text: "obop does not check that the drawer was counted." },
      {
        slug: "accounting",
        text: "An entry with no lines has debits and credits of zero, which are equal, so it can be posted.",
      },
      {
        slug: "contacts",
        text: "The erase does not touch the list of placeholders an AI agent holds;",
      },
      {
        slug: "planning",
        text: "No action creates a shift from a template, and a shift has no field that names one.",
      },
    ];
    for (const { slug, text } of folded) {
      const html = await (await request.get(`${base()}/apps/${slug}/`)).text();
      const fold = html.indexOf('<details id="developers"');
      expect(fold, `${slug}: the fold`).toBeGreaterThan(0);
      expect(html.indexOf(text), `${slug}: in the page, after the fold opens`).toBeGreaterThan(
        fold,
      );
      expect(html.slice(0, fold), `${slug}: not above the fold`).not.toContain(text);
      const md = await (await request.get(`${base()}/apps/${slug}.md`)).text();
      const cut = md.indexOf("\n## For developers\n");
      expect(cut, `${slug}: the copy's fold`).toBeGreaterThan(0);
      expect(md.indexOf(text), `${slug}: in the copy, under For developers`).toBeGreaterThan(cut);
      // Under the package heading, the first of the folded part.
      expect(md.indexOf(text), `${slug}: under the package text`).toBeLessThan(
        md.indexOf("\n### Module details\n", cut),
      );
    }
    // What stays on top says what holds without them.
    const eco = await (await request.get(`${base()}/apps/ecommerce.md`)).text();
    expect(eco.slice(0, eco.indexOf("\n## For developers\n"))).toContain(
      "the order stays paid if Stripe refuses. Otherwise you pay it back yourself.",
    );
    const pos = await (await request.get(`${base()}/apps/pos.md`)).text();
    expect(pos.slice(0, pos.indexOf("\n## For developers\n"))).toContain(
      "a paid sale can still be edited afterwards",
    );
    // On a translated page the list is English, inside the English note.
    const de = await (await request.get(`${base()}/de/apps/pos/`)).text();
    const text = "The payments are checked only when the sale is paid.";
    const at = de.indexOf(text);
    expect(at, "the sentence is on the German page").toBeGreaterThan(
      de.indexOf('<details id="developers"'),
    );
    const note = de.lastIndexOf('class="en-note"', at);
    expect(note, "an English note opens before it").toBeGreaterThan(-1);
    expect(de.slice(note, at), "and is still open").not.toContain("</div>");
    expect(de.slice(note - 40, note), "marked as English").toContain('lang="en"');
    // The Template card of Planning does not say, on a translated page either, that a template is
    // copied into shifts (the product's German text does).
    const plan = await (await request.get(`${base()}/de/apps/planning/`)).text();
    const sentence = "A shift pattern, with a name, start and end times and, optionally, a role.";
    const card = plan.indexOf(sentence);
    expect(card, "the English card is on the German page").toBeGreaterThan(0);
    expect(plan.slice(card - 40, card), "and is marked as English").toContain('lang="en">');
    expect(plan).not.toContain("das in Schichten kopiert wird");
    // The contact's erase says how long an agent's placeholder stands, for the obop mcp command too.
    expect(await (await request.get(`${base()}/apps/contacts.md`)).text()).toContain(
      "through the obop mcp command, the placeholder keeps standing until that command stops.",
    );
  });

  test("the site says obop is below version 1.0, not beta, in the footer, the home page pill and llms.txt", async ({
    request,
  }) => {
    const txt = await (await request.get(`${base()}/llms.txt`)).text();
    expect(txt).toContain("Status: below version 1.0");
    expect(txt).not.toContain("Status: beta");
    const home = await (await request.get(`${base()}/`)).text();
    expect(home).toMatch(/class="pill"[^>]*><i><\/i>\s*Pre-1\.0/);
    expect(home).not.toMatch(/class="pill"[^>]*><i><\/i>\s*Beta/);
    expect(home).toContain("Open-source business platform. Below version 1.0.");
  });

  // The strings are pinned in the site's unit tests (strings.test.ts); this reads the pages as built, so a
  // footer or a pill that is drawn from somewhere else than the language's own strings is found too.
  test("every language's home page says in its own words that obop is below version 1.0, in the footer and the pill", async ({
    request,
  }) => {
    const old = /beta|bêta/i;
    for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
      const m = messages[locale];
      const html = await (
        await request.get(`${base()}${locale === "en" ? "" : `/${locale}`}/`)
      ).text();
      const pill = /class="pill"[^>]*><i><\/i>\s*([^<]*)</.exec(html)?.[1]?.trim();
      expect(pill, `${locale}: the pill`).toBe(m.home.badge);
      expect(pill, `${locale}: the pill`).not.toMatch(old);
      const footer = /<\/a><p>([^<]*)<\/p><p class="fine"/.exec(html)?.[1];
      expect(footer, `${locale}: the footer note`).toBe(m.ui.betaNote);
      expect(footer, `${locale}: the footer note`).not.toMatch(old);
    }
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
    // There is one obop and no edition: an app's offer is free and says nothing else (a sentence
    // about "the open-source edition" was English on every translated page, and implied a second one).
    for (const path of ["/apps/crm/", "/apps/helpdesk/", "/sv/apps/sign/"]) {
      await page.goto(`${base()}${path}`);
      const ld = (await page.locator('script[type="application/ld+json"]').allTextContents()).map(
        (r) => JSON.parse(r),
      );
      const app = ld.find((x) => x["@type"] === "SoftwareApplication");
      expect(app.offers, `${path}: the offer`).toEqual({
        "@type": "Offer",
        price: "0",
        priceCurrency: "EUR",
      });
    }
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

  // The two walks below visit every apps page and take ten to fifty seconds (the Tab walk the most), where
  // the repository's limit is 30 s a test. A limit set in a test's body (test.setTimeout) does not cover
  // its fixtures, and one slow start of a page in system Chrome used up the default before the body began:
  // so the limit is set for the group, like that of the sweeps further down.
  test.describe("walks over the apps pages", () => {
    test.describe.configure({ timeout: 240_000 });

    test("every apps page has the same sound structure, in its own language, with one small script", async ({
      page,
      request,
    }) => {
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
            document
              .querySelectorAll("[id]")
              .forEach((e) => ids.set(e.id, (ids.get(e.id) ?? 0) + 1));
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
            // The walk ends when every control of main has had its turn (plus the header's and the
            // footer's on the way), not after a fixed number of presses: this is the slowest test of
            // the file, and a slower runner has more room that way.
            const expected = await page.evaluate(
              () =>
                [...document.querySelectorAll("main a[href], main summary, main button")].filter(
                  (el) => el.getClientRects().length > 0 && el.checkVisibility(),
                ).length,
            );
            for (let i = 0; i < 120 && stops < expected; i++) {
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
    // There is no hover to reveal the copy button of the install box: it is drawn, not transparent.
    for (const path of ["/apps/helpdesk/", "/sv/apps/sign/"]) {
      await page.goto(`${base()}${path}`);
      const copy = await page.locator(".app-getbox button.copy").evaluate((el) => ({
        opacity: getComputedStyle(el).opacity,
        hover: matchMedia("(hover: none)").matches,
      }));
      expect(copy, `${path} copy button on a touch screen`).toEqual({ opacity: "1", hover: true });
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
        // The names that broke in the middle of a word at 150 and 200 percent text on a 320 px phone.
        "/apps/manufacturing/",
        "/apps/appointments/",
        "/apps/subscriptions/",
        "/apps/connectors/",
        "/apps/dashboards/",
        "/apps/documents/",
        "/apps/ecommerce/",
        "/apps/knowledge/",
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
        // Hyphenation is switched off, so that the result is the same on every browser.
        await p.addStyleTag({ content: "* { hyphens: manual !important; }" });
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
        // The name is never cut inside a word that fits the column: at 320 px and 200 percent too
        // (the name shrinks with the room it has), and for the eight names that once broke there.
        // Only a word wider than the column even at the smallest size (one the name may have) breaks.
        const { cut } = await brokenWords(p, ".app-hero h1", true);
        expect(cut, `${where}: the name breaks inside a word that fits`).toEqual([]);
        if (path === "/apps/accounting/")
          expect(over.lines, `${where}: the name breaks inside the word`).toBe(over.words);
      }
      await context.close();
    }
  });

  test("a long page name carries the seams of its compound, keeps its size, and is still the plain name to a reader", async ({
    page,
    request,
  }) => {
    // The index had soft hyphens at the seams of these names; the page's own name did not, so at 150
    // and 200 percent text on a phone it was cut inside a word ("Sähköpostimarkkinoin" / "ti", "E-" /
    // "postmarkn" / "adsföring") and set smaller than the section headings could follow.
    await page.setViewportSize({ width: 320, height: 800 });
    for (const [path, name] of [
      ["/de/apps/invoicing/", "Rechnungsstellung"],
      ["/fi/apps/email-marketing/", "Sähköpostimarkkinointi"],
      ["/fi/apps/inventory/", "Varastonhallinta"],
      ["/sv/apps/email-marketing/", "E-postmarknadsföring"],
      ["/sv/apps/dashboards/", "Instrumentpaneler"],
    ] as const) {
      await page.goto(`${base()}${path}`);
      const h1 = page.locator(".app-hero h1");
      const text = (await h1.textContent())!;
      expect(text, `${path}: soft hyphens at the seams`).toContain("\u00ad");
      expect(text.replaceAll("\u00ad", ""), `${path}: nothing but soft hyphens added`).toBe(name);
      // The name is not set smaller on a phone: it is the same size as any other page's.
      expect(
        parseFloat(await h1.evaluate((el) => getComputedStyle(el).fontSize)),
        `${path}: the name's size`,
      ).toBeGreaterThanOrEqual(30);
      // The title, the Markdown copy and the search index keep the plain name.
      expect(await page.title(), `${path}: title`).toContain(name);
      const md = await (await request.get(`${base()}${path.replace(/\/$/, "")}.md`)).text();
      expect(md, `${path}: the Markdown copy`).toContain(`title: "${name}`);
      expect(md, `${path}: the Markdown copy has no soft hyphen`).not.toContain("\u00ad");
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
    // Printed (at the width of an A4 page): the folds print open and their buttons are not printed
    // (the content they would open is already there), and neither the hero's button nor the copy
    // buttons are printed.
    await page.setViewportSize({ width: 794, height: 1123 });
    await page.goto(`${base()}/apps/accounting/`);
    await page.emulateMedia({ forcedColors: "none", media: "print" });
    await expect(page.locator(".app-primary")).toBeHidden();
    await expect(page.locator("button.copy").first()).toBeHidden();
    await expect(page.locator("details#developers")).toHaveJSProperty("open", false);
    await expect(page.getByRole("heading", { level: 3, name: "Models" })).toBeVisible();
    await expect(page.locator(".app-dev > summary")).toBeHidden();
    await expect(page.locator(".app-more > summary").first()).toBeHidden();
    await page.goto(`${base()}/apps/connectors/`);
    await expect(page.locator(".app-read > summary")).toBeHidden();
    // The folds still open on screen: the button is only gone from the printed page.
    await page.emulateMedia({ media: "screen" });
    await expect(page.locator(".app-read > summary")).toBeVisible();
  });

  test("a printed app page is as wide as the paper: no table is wider than the sheet, so none is shrunk", async ({
    page,
  }) => {
    // A table wider than A4 (794 px) is scaled down by the browser to about 7 pt; the technical
    // tables must instead share the width of the paper. Measured as the page's own scroll width in
    // print, in the languages with the longest words, on the pages with the widest tables.
    await page.setViewportSize({ width: 794, height: 1123 });
    await page.emulateMedia({ media: "print" });
    for (const path of [
      "/apps/connectors/",
      "/apps/hr/",
      "/apps/ecommerce/",
      "/apps/accounting/",
      "/apps/sign/",
      "/de/apps/ecommerce/",
      "/de/apps/hr/",
      "/de/apps/accounting/",
      "/de/apps/email-marketing/",
    ]) {
      await page.goto(`${base()}${path}`);
      const m = await page.evaluate(() => ({
        page: document.documentElement.scrollWidth,
        paper: window.innerWidth,
        tables: [...document.querySelectorAll("#developers table")].filter(
          (t) => t.getBoundingClientRect().right > window.innerWidth + 0.5,
        ).length,
      }));
      expect(m.page, `${path}: the printed page is wider than the sheet`).toBeLessThanOrEqual(
        m.paper,
      );
      expect(m.tables, `${path}: tables that run past the sheet`).toBe(0);
    }
  });

  test("a printed app page keeps each card, line, information row, list item and table row whole, a heading with what follows it, and the plain words of the tables", async ({
    page,
  }) => {
    // The sheets are cut by the browser's own pagination, which a test cannot see; what the page
    // asks of it is read from the style that applies in print, on the elements that have to stay
    // together. The words of the tables are measured as they are drawn at the width of A4.
    await page.setViewportSize({ width: 794, height: 1123 });
    await page.emulateMedia({ media: "print" });
    await page.goto(`${base()}/apps/crm/`);
    const asked = await page.evaluate(() => {
      const of = (sel: string, prop: "breakInside" | "breakAfter") => {
        const els = [...document.querySelectorAll(sel)];
        return {
          count: els.length,
          values: [...new Set(els.map((e) => getComputedStyle(e)[prop]))],
        };
      };
      return {
        cards: of(".app-card", "breakInside"),
        safe: of(".app-safe li", "breakInside"),
        icons: of(".app-icons li", "breakInside"),
        info: of(".app-info > div", "breakInside"),
        // A row of a technical table: "or payee" once stood alone at the top of a sheet, in an
        // otherwise empty row under the repeated header, after "Bank account of the payer" at the foot of the last.
        rows: of(".app-dev .prose tr", "breakInside"),
        // An item of a list in the part for developers (the caveats under the package text, the
        // permission groups): a sentence is not cut across two sheets.
        items: of(".app-dev .prose li", "breakInside"),
        h2: of(".app-page h2", "breakAfter"),
        h3: of(".app-dev .prose h3", "breakAfter"),
        h4: of(".app-dev .prose h4", "breakAfter"),
        // The description and the "Read group" line of a model, which stay with its table (they were
        // left at the foot of a sheet, with the table on the next), and the lines under "Permission groups".
        underModel: of(".app-dev .prose h4 + p, .app-dev .prose h4 + p + p", "breakAfter"),
        underHeading: of(".app-dev .prose h3 + p, .app-dev .prose h3 + p + p", "breakAfter"),
      };
    });
    for (const [name, r] of Object.entries(asked)) {
      expect(r.count, `${name}: elements measured`).toBeGreaterThan(0);
      expect(r.values, `${name}: how it is allowed to break`).toEqual(["avoid"]);
    }
    // No word of a cell is split ("masked" once printed as "maske / d", the data class "none" as
    // "non / e" and the group "connectors.user" as "connec / tors.us / er"): a code token breaks only
    // after an underscore or a dot, where the page gives it room to, and every part of it stays
    // whole. No word is allowed to be wider than its cell here: a column that shrinks below its word
    // is exactly the defect (the cell is as narrow as the broken word needs). A4 is 794 px wide, and
    // 680 px inside the usual 15 mm margins.
    for (const sheet of [794, 680]) {
      await page.setViewportSize({ width: sheet, height: 1123 });
      for (const path of [
        "/apps/crm/",
        "/apps/connectors/",
        "/apps/hr/",
        "/apps/ecommerce/",
        "/apps/accounting/",
        "/apps/sign/",
        "/de/apps/ecommerce/",
        "/de/apps/hr/",
        "/de/apps/accounting/",
        "/fi/apps/accounting/",
        "/sv/apps/connectors/",
      ]) {
        await page.goto(`${base()}${path}`);
        const { cut } = await brokenWords(
          page,
          ".app-dev .prose th, .app-dev .prose td",
          false,
          "",
          true,
        );
        expect(cut, `${path} on ${sheet}px paper: a word of a table cell is split`).toEqual([]);
      }
    }
  });

  // The sweeps below visit every page of every language at several widths and text sizes, in a
  // browser whose text size is set through the protocol. Their time limit is set for the whole
  // group, not by test.setTimeout in each body: a limit set in the body cannot cover the fixtures,
  // and a slow start of a page (system Chrome, after another sweep) counted against the default 30 s
  // and failed a test that had not begun. The limit is room, not a fix for a slow sweep. The slowest
  // (the facts strip) makes about 2,900 round trips to the browser for its 49 distinct pages: measured
  // alone, it takes 29 s in the bundled Chromium and 36 s in system Chrome (12 ms a round trip), and 44 s
  // in system Chrome while 32 busy loops took every thread of the machine. A review saw 114 s and 282 s
  // in system Chrome once each and the cause was not found; it was not Chrome's speed on these
  // operations (page loads, resizes and range measurements cost the same in both) and it was not
  // the load of the machine, so a slow run is something to look into, not to wait for.
  test.describe("sweeps over every page, language and size", () => {
    test.describe.configure({ timeout: 900_000 });

    test("the apps index keeps its text readable at a large text size on a narrow screen, in every language", async ({
      browser,
    }) => {
      // The browser's own text size is set (rem and rem media queries both scale; a style rule on the
      // root element would leave the media queries at 16 px). The icon of a tile or a row grows with
      // the text, so beside it the text column would shrink to a few letters per line: from a text
      // column of less than 8rem the text must drop under the icon instead. A word that fits its
      // column is never cut; only a word wider than the column may be, and the words that are, at
      // each size, are listed here: nothing else may be on the list (an ordinary name such as English
      // "Connectors" once broke after "Connecto" at 200 percent on 320 px). The five long compound names
      // (German "Rechnungsstellung", Finnish "Sähköpostimarkkinointi", Swedish "E-postmarknadsföring" and
      // the rest) are not on it: they have seams (softname.ts), a soft hyphen at the joints of the
      // compound, so a line that is too short for one breaks it there and the browser never cuts it
      // inside a word; they were on this list, three of them at 125 percent text on a 375 px phone.
      // What remains is one Finnish tagline word.
      const indexes = Object.keys(messages).map((l) => (l === "en" ? "/apps/" : `/${l}/apps/`));
      const widest: Record<string, string[]> = {
        "640@200": [],
        "700@200": [],
        "390@150": [],
        "390@200": [],
        "375@200": [],
        "375@125": ["myyntimahdollisuuksia"],
        "360@175": [],
        "320@150": [],
        "320@200": [],
      };
      for (const [width, percent] of [
        [640, 200],
        [700, 200],
        [390, 150],
        [390, 200],
        [375, 200],
        [375, 125],
        [360, 175],
        [320, 150],
        [320, 200],
      ] as const) {
        const { page, context } = await pageWithTextSize(browser, percent, { width, height: 900 });
        try {
          for (const path of indexes) {
            await page.goto(`${base()}${path}`);
            // Hyphenation is switched off, so that the result is the same on every browser.
            await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
            const where = `${path} at ${width}px with ${percent}% text`;
            const m = await page.evaluate(() => {
              const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
              // The page's own content (the shared header has its own limits at such sizes).
              const store = document.querySelector(".store") as HTMLElement;
              const squeezed: string[] = [];
              const items = [...document.querySelectorAll(".store-tile, .store-row")];
              for (const item of items) {
                const icon = item.querySelector(".app-icon")!.getBoundingClientRect();
                const text = item.querySelector(".store-text")!.getBoundingClientRect();
                if (text.top < icon.bottom - 0.5 && text.width < 7.9 * rem)
                  squeezed.push(
                    `${item.querySelector(".store-name")!.textContent} (${(text.width / rem).toFixed(1)}rem)`,
                  );
              }
              return {
                page: store.scrollWidth - store.clientWidth,
                clipped: [...document.querySelectorAll(".store-text")].filter(
                  (e) => e.scrollWidth > e.clientWidth + 1,
                ).length,
                items: items.length,
                squeezed,
              };
            });
            expect(m, where).toEqual({ page: 0, clipped: 0, items: 28, squeezed: [] });
            const names = await brokenWords(page, ".store-name, .store-tag", true);
            expect(names.cut, `${where}: a word that fits its column is cut`).toEqual([]);
            expect(
              names.wideWords.filter((w) => !widest[`${width}@${percent}`]!.includes(w)),
              `${where}: words wider than their column that are not on the list`,
            ).toEqual([]);
            // The headings shrink with the room they have, so none of their words is cut at all.
            const headings = await brokenWords(
              page,
              ".store-head h1, .store-sec > h2, .store-group h3",
            );
            expect(headings.cut, `${where}: a heading is cut in the middle of a word`).toEqual([]);
          }
        } finally {
          await context.close();
        }
      }
    });

    test("at the normal text size the text of a tile or a row sits beside its icon on every phone and tablet, and no name is cut", async ({
      page,
    }) => {
      // The tile's text drops under the icon only when the column beside it would be narrower than
      // 8rem (128 px), so at 16 px text it does so at no width: the three-column grid at its
      // narrowest (888 to 912 px, a 272 px column) once left 152 px beside the icon and flipped.
      // From 64rem a tile is a column on purpose (icon above text), so it is left out there.
      for (const width of [
        320, 360, 375, 414, 600, 640, 700, 768, 800, 888, 896, 904, 912, 960, 1000,
      ]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto(`${base()}/apps/`);
        const stacked = await page.evaluate(() =>
          [...document.querySelectorAll(".store-tile, .store-row")]
            .filter((item) => {
              const icon = item.querySelector(".app-icon")!.getBoundingClientRect();
              const text = item.querySelector(".store-text")!.getBoundingClientRect();
              return text.top >= icon.bottom - 0.5;
            })
            .map((item) => item.querySelector(".store-name")!.textContent),
        );
        expect(
          stacked,
          `/apps/ at ${width}px: text under its icon at the normal text size`,
        ).toEqual([]);
      }
      // The narrowest phone: the longest name of each language (German "Rechnungsstellung" in a tile,
      // Finnish "Sähköpostimarkkinointi" in a row) is whole, in every language, at 320 and 360 px.
      for (const width of [320, 360]) {
        await page.setViewportSize({ width, height: 900 });
        for (const locale of Object.keys(messages)) {
          await page.goto(`${base()}${locale === "en" ? "" : `/${locale}`}/apps/`);
          await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
          const { cut } = await brokenWords(page, ".store-name, .store-tag");
          expect(cut, `${locale} index at ${width}px`).toEqual([]);
        }
      }
    });

    test("the index names hyphenate in the languages that make long compounds, and stay whole in English", async ({
      page,
    }) => {
      for (const locale of Object.keys(messages)) {
        await page.goto(`${base()}${locale === "en" ? "" : `/${locale}`}/apps/`);
        const hyphens = await page.evaluate(() => [
          ...new Set(
            [...document.querySelectorAll(".store-name, .store-tag")].map(
              (e) => getComputedStyle(e).hyphens,
            ),
          ),
        ]);
        expect(hyphens, locale).toEqual(
          ["de", "fi", "pl", "sv"].includes(locale) ? ["auto"] : ["manual"],
        );
      }
    });

    test("the header button is the plain one on the apps index too, 'Read more' has air above it, and a right-to-left page mirrors the chevrons", async ({
      page,
    }) => {
      // One filled pill per screen: on the apps pages (the index has none of its own) the header's
      // "Start free" is the plain button, and on the home page it stays the filled one.
      const header = () =>
        page.locator(".head-cta .btn.primary").evaluate((el) => {
          const cs = getComputedStyle(el);
          return { background: cs.backgroundColor, color: cs.color, border: cs.borderTopColor };
        });
      await page.goto(`${base()}/apps/`);
      const onIndex = await header();
      await page.goto(`${base()}/apps/helpdesk/`);
      const onApp = await header();
      await page.goto(`${base()}/`);
      const onHome = await header();
      expect(onIndex, "the header button on /apps/ and on an app page").toEqual(onApp);
      expect(onIndex, "the header button on /apps/ and on the home page").not.toEqual(onHome);

      // The fold under a long purpose does not touch the sentence above it.
      for (const width of [375, 1280]) {
        await page.setViewportSize({ width, height: 900 });
        await page.goto(`${base()}/apps/connectors/`);
        // From the paragraph right above the fold (the caveat of the page), whatever it is.
        const gap = await page.evaluate(
          () =>
            document.querySelector(".app-read > summary")!.getBoundingClientRect().top -
            document.querySelector(".app-read")!.previousElementSibling!.getBoundingClientRect()
              .bottom,
        );
        expect(gap, `space above "Read more" at ${width}px`).toBeGreaterThanOrEqual(12);
      }

      // Right-to-left is not offered, but the back and row chevrons already point the other way.
      for (const [path, chevron] of [
        ["/apps/crm/", ".app-back svg"],
        ["/apps/", ".store-chev svg"],
      ] as const) {
        await page.goto(`${base()}${path}`);
        // The built style sheet turns :dir(rtl) into the list of right-to-left languages (:lang(ar),
        // ...), for browsers that lack :dir(), so a right-to-left page is one with both set.
        const turn = (dir: string, lang: string) =>
          page.evaluate(
            ([d, l, sel]) => {
              document.documentElement.dir = d!;
              document.documentElement.lang = l!;
              return getComputedStyle(document.querySelector(sel!)!).transform;
            },
            [dir, lang, chevron],
          );
        expect(await turn("ltr", "en"), `${path} chevron, left to right`).toBe("none");
        expect(await turn("rtl", "ar"), `${path} chevron, right to left`).toBe(
          "matrix(-1, 0, 0, 1, 0, 0)",
        );
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

    test("no name or label is cut in the middle of a word on a phone, in any language", async ({
      page,
      request,
    }) => {
      // The hero's name, the icon names and the information labels (the facts strip and the cards have
      // tests of their own below: this one cannot see a box that grows past its cell).
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
            await settle(page);
            const cut = await page.evaluate(() =>
              [...document.querySelectorAll(".app-hero h1, .app-icons a, .app-info dt")]
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

    test("at any text size the page name is larger than every section heading, and no card title, technical heading, tagline or large sentence is larger than what it sits under", async ({
      browser,
    }) => {
      // At 150 percent text the section headings were larger than the page name (the heading's size
      // shrank less with the room than the name's did), and at 200 percent on 320 px the large sentence
      // was too. The headings and the sentence are now sizes derived from the name, so this holds at
      // every text size and width, on the index and on app pages (long names included).
      const pages = [
        "/apps/",
        "/apps/accounting/",
        "/apps/crm/",
        "/de/apps/invoicing/",
        "/fi/apps/email-marketing/",
        "/sv/apps/dashboards/",
      ];
      let measured = 0;
      for (const percent of [100, 125, 150, 200]) {
        const { page, context } = await pageWithTextSize(browser, percent);
        try {
          for (const width of [320, 375, 390, 768, 1280]) {
            await page.setViewportSize({ width, height: 900 });
            for (const path of pages) {
              await page.goto(`${base()}${path}`, { waitUntil: "domcontentloaded" });
              const size = await page.evaluate(() => {
                const px = (el: Element | null) =>
                  el ? parseFloat(getComputedStyle(el).fontSize) : 0;
                return {
                  name: px(document.querySelector(".app-hero h1, .store-head h1")),
                  headings: [...document.querySelectorAll(".app-page h2, .store-sec > h2")].map(px),
                  lead: px(document.querySelector(".app-purpose, .store-lead")),
                  // The line under the name (the page's own, absent on the index), the titles of the
                  // cards, and the headings of the technical part, which is closed (a closed fold still has
                  // computed sizes).
                  tagline: px(document.querySelector(".app-tagline")),
                  cards: [...document.querySelectorAll(".app-card h3")].map(px),
                  dev3: [...document.querySelectorAll(".app-dev .prose h3")].map(px),
                  dev4: [...document.querySelectorAll(".app-dev .prose h4")].map(px),
                };
              });
              const where = `${path} at ${width}px with ${percent}% text`;
              expect(size.headings.length, `${where}: section headings`).toBeGreaterThan(1);
              expect(
                Math.max(...size.headings),
                `${where}: the largest section heading`,
              ).toBeLessThan(size.name);
              // And visibly so on an app page up to 150 percent text: a long name was set smaller on a
              // phone and was 3 percent larger than the headings (24.3 px against 23.6) at 100 percent.
              if (path !== "/apps/" && percent <= 150)
                expect(
                  size.name,
                  `${where}: the name is clearly larger than the section headings`,
                ).toBeGreaterThanOrEqual(Math.max(...size.headings) * 1.15);
              expect(size.lead, `${where}: the large sentence`).toBeLessThanOrEqual(size.name);
              expect(size.tagline, `${where}: the tagline`).toBeLessThanOrEqual(size.name);
              // A card title is under the heading of its section (it was larger at 125 percent on a 320 px
              // phone, and larger than the name at 200), and so is a heading of the technical part; a
              // heading under that is under it. They are also never below the size of the text.
              const heading = Math.min(...size.headings);
              if (percent === 100) {
                expect(
                  heading,
                  `${where}: a section heading is clearly larger than the large sentence`,
                ).toBeGreaterThanOrEqual(size.lead * 1.1);
                for (const px of size.cards)
                  expect(
                    px,
                    `${where}: a card title is clearly under the section heading`,
                  ).toBeLessThanOrEqual(heading * 0.95);
              }
              for (const px of size.cards) {
                expect(px, `${where}: a card title and the section heading`).toBeLessThanOrEqual(
                  heading + 0.01,
                );
                expect(px, `${where}: a card title and the name`).toBeLessThan(size.name);
              }
              for (const px of size.dev3)
                expect(
                  px,
                  `${where}: a technical heading and the section heading`,
                ).toBeLessThanOrEqual(heading + 0.01);
              for (const px of size.dev4)
                expect(
                  px,
                  `${where}: a technical sub-heading and the heading above it`,
                ).toBeLessThanOrEqual(Math.min(...size.dev3) + 0.01);
              if (path !== "/apps/") {
                expect(size.cards.length, `${where}: card titles measured`).toBeGreaterThan(0);
                expect(size.dev3.length, `${where}: technical headings measured`).toBeGreaterThan(
                  0,
                );
                expect(
                  size.dev4.length,
                  `${where}: technical sub-headings measured`,
                ).toBeGreaterThan(0);
              }
              measured++;
            }
          }
        } finally {
          await context.close();
        }
      }
      expect(measured).toBe(4 * 5 * pages.length);
    });

    test("the install command of a marketplace app keeps a column of its own at a large text size on a phone, and the copy button does not sit on it", async ({
      browser,
    }) => {
      // The button's room was reserved beside the command, in rem: at 200 percent on 320 px the column
      // was 28 px wide and the command broke into single letters (34 lines, a box 1,479 px high).
      for (const [width, percent] of [
        [320, 200],
        [320, 150],
        [320, 125],
        [375, 200],
        [375, 125],
        [390, 100],
        [640, 200],
      ] as const) {
        const { page, context } = await pageWithTextSize(browser, percent, { width, height: 900 });
        try {
          for (const path of [
            "/apps/accounting/",
            "/fi/apps/email-marketing/",
            "/de/apps/field-service/",
          ]) {
            await page.goto(`${base()}${path}`);
            await page.waitForSelector("html.js-ready");
            await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
            const where = `${path} at ${width}px with ${percent}% text`;
            const m = await page.evaluate(() => {
              const code = document.querySelector(".app-getbox pre code")!;
              const range = document.createRange();
              range.selectNodeContents(code);
              const rects = [...range.getClientRects()].filter((r) => r.width > 0);
              const button = document
                .querySelector(".app-getbox button.copy")!
                .getBoundingClientRect();
              return {
                lines: new Set(rects.map((r) => Math.round(r.top))).size,
                onButton: rects.some(
                  (r) =>
                    r.right > button.left &&
                    r.left < button.right &&
                    r.bottom > button.top &&
                    r.top < button.bottom,
                ),
                box: document
                  .querySelector(".app-getbox div[class*='language-']")!
                  .getBoundingClientRect().height,
              };
            });
            expect(m.lines, `${where}: lines of the command`).toBeLessThanOrEqual(8);
            expect(m.onButton, `${where}: the command runs under the copy button`).toBe(false);
            // Not `allowWide`: the command is an inline element, which has no width of its own, so every
            // word looked wider than "its box" and none was ever reported. The box is the line of the
            // block, and a word of up to 12 letters (obop, modules, install-from, most slugs) must fit it.
            const { cut } = await brokenWords(page, ".app-getbox pre code");
            expect(
              cut.filter((c) => c.split(" ")[0]!.length <= 12),
              `${where}: a word of the command is cut`,
            ).toEqual([]);
            const lead = await brokenWords(page, ".app-getbox p", true);
            expect(lead.cut, `${where}: a word of the sentences in the box is cut`).toEqual([]);
          }
        } finally {
          await context.close();
        }
      }
    });

    test("the page name and the section headings keep their words whole at a larger text size, in every language", async ({
      request,
      browser,
    }) => {
      // The browser's own text size is set to 125, 150 and 200 percent (rem units and rem media
      // queries scale), on 320 and 375 px phones, and every heading of every apps page is asked which
      // of its words were drawn on two lines. A heading shrinks with the room it has (a vw term over a
      // low rem floor), so a word that fits stays whole; a word wider than the column even at the
      // smallest size a heading may have cannot be kept whole by any size and breaks at the column's
      // edge. Those words are listed here, and nothing else may be on the list. (Hyphenation is
      // switched off, so that the result is the same on every browser.) The list is empty: the five
      // long page names carry the seams of their compound (softname.ts), like the names on the index, so
      // the browser breaks them at a joint, with a hyphen, and never inside a word; they were on this
      // list at 150 and 200 percent text, and the headings are no wider than a column at any size.
      const widest: Record<string, string[]> = {
        "125@320": [],
        "125@375": [],
        "150@375": [],
        "150@320": [],
        "200@375": [],
        "200@320": [],
      };
      const seen: Record<string, Set<string>> = Object.fromEntries(
        Object.keys(widest).map((k) => [k, new Set<string>()]),
      );
      const sizes = new Map<number, Awaited<ReturnType<typeof pageWithTextSize>>>();
      for (const percent of [125, 150, 200])
        sizes.set(percent, await pageWithTextSize(browser, percent));
      let pages = 0;
      try {
        for (const locale of Object.keys(messages)) {
          for (const url of (await pagesOf(request, locale)).filter((u) =>
            /\/apps\/.+\/$/.test(u),
          )) {
            for (const [percent, { page }] of sizes) {
              await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
              await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
              for (const width of [320, 375]) {
                await page.setViewportSize({ width, height: 900 });
                const { cut, wideWords } = await brokenWords(
                  page,
                  ".app-hero h1, .app-page h2",
                  true,
                );
                const where = `${url} at ${width}px with ${percent}% text`;
                expect(cut, `${where}: a heading is cut inside a word that fits`).toEqual([]);
                for (const w of wideWords) seen[`${percent}@${width}`]!.add(w);
              }
            }
            pages++;
          }
        }
        expect(pages).toBe(7 * 28);
        for (const [key, allowed] of Object.entries(widest)) {
          const extra = [...seen[key]!].filter((w) => !allowed.includes(w));
          expect(extra, `${key}: words wider than the column that are not on the list`).toEqual([]);
        }
        // Not vacuous: the measurement can fail. Without its seams the longest name is wider than its
        // column at 200 percent on the narrowest phone, and the test sees it (the page name used to be
        // drawn so).
        const { page: probe } = sizes.get(200)!;
        await probe.setViewportSize({ width: 320, height: 900 });
        await probe.goto(`${base()}/de/apps/invoicing/`, { waitUntil: "domcontentloaded" });
        await probe.addStyleTag({ content: "* { hyphens: manual !important; }" });
        await probe.evaluate(() => {
          const h1 = document.querySelector(".app-hero h1")!;
          h1.textContent = (h1.textContent ?? "").replaceAll("­", "");
        });
        const bare = await brokenWords(probe, ".app-hero h1", true);
        expect(bare.wideWords, "the name without its seams").toEqual(["Rechnungsstellung"]);
      } finally {
        for (const { context } of sizes.values()) await context.close();
      }
    });

    test("the facts strip keeps every word whole and inside its cell, in the layout its room asks for, in every language, as the page is drawn", async ({
      page,
      request,
      browser,
    }) => {
      // Measured with nothing switched off: each word of each label and value is asked how many lines
      // it was drawn on, and where it sits compared with its cell. (The strip once cut "Experimental"
      // into "Experiment" and "al" between 640 and 810 px, and "Eksperymentalny" up to 800 px, while a
      // test that first switched the safety nets off passed: the cell had shrunk below the word, which
      // that state hid. A word that a cell is too small for is not cut when overflow-wrap is break-word:
      // the box grows past its cell, so the words are also asked to lie inside it.) Every maturity word
      // and the "no other apps" line of every language is tried, at the widths where the layout
      // changes, and at larger text sizes set in the browser, as a visitor sets them.
      const widths = [320, 360, 375, 414, 480, 600, 640, 700, 768, 800, 860, 900, 1024, 1280];
      const large = [
        [320, 200],
        [375, 150],
        [640, 200],
        [900, 150],
      ] as const;
      const sizes = new Map<number, Awaited<ReturnType<typeof pageWithTextSize>>>();
      for (const percent of new Set(large.map(([, p]) => p)))
        sizes.set(percent, await pageWithTextSize(browser, percent));

      /** What the strip looks like now: words cut, text outside its cell, sideways scroll, columns. */
      const measure = async (p: import("@playwright/test").Page) => {
        const { cut } = await brokenWords(p, ".app-facts dt, .app-facts dd");
        const layout = await p.evaluate(() => {
          const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
          const outside: string[] = [];
          const cells = [...document.querySelectorAll(".app-facts > div")];
          for (const cell of cells) {
            const box = cell.getBoundingClientRect();
            for (const el of cell.querySelectorAll("dt, dd")) {
              const range = document.createRange();
              range.selectNodeContents(el);
              for (const r of range.getClientRects())
                if (r.width > 0 && (r.left < box.left - 0.5 || r.right > box.right + 0.5))
                  outside.push(
                    `${(el.textContent ?? "").trim()} (${Math.round(r.left)}..${Math.round(r.right)} in ${Math.round(box.left)}..${Math.round(box.right)})`,
                  );
            }
          }
          const root = document.querySelector(".app-page") as HTMLElement;
          return {
            outside,
            cells: cells.length,
            sideways: root.scrollWidth - root.clientWidth,
            columns: new Set(cells.map((c) => Math.round(c.getBoundingClientRect().left))).size,
            room: (document.querySelector(".app-facts-box") as HTMLElement).clientWidth / rem,
          };
        });
        return { cut, ...layout };
      };
      /** The columns the room asks for: one below 17.5rem, two by two, one row of four from 52rem. */
      const columnsFor = (room: number) => (room < 17.5 ? 1 : room < 52 ? 2 : 4);

      try {
        const seenColumns = new Set<number>();
        for (const locale of Object.keys(messages) as (keyof typeof messages)[]) {
          const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u));
          const seen = new Set<string>();
          const maturities = new Set<string>();
          let standalone = 0;
          for (const url of urls) {
            await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
            const values = await page.locator(".app-facts dd").allTextContents();
            const key = `${values[0]}|${values[2] === messages[locale].apps.store.standalone}`;
            if (seen.has(key)) continue;
            seen.add(key);
            maturities.add(values[0]!);
            if (values[2] === messages[locale].apps.store.standalone) standalone++;
            const check = async (p: import("@playwright/test").Page, where: string) => {
              const r = await measure(p);
              expect(r.cut, `${locale} ${url} ${where}: a word is cut`).toEqual([]);
              expect(r.outside, `${locale} ${url} ${where}: text outside its cell`).toEqual([]);
              expect(
                r.sideways,
                `${locale} ${url} ${where}: the page scrolls sideways`,
              ).toBeLessThanOrEqual(0);
              expect(r.cells, `${locale} ${url} ${where}: four cells`).toBe(4);
              expect(
                r.columns,
                `${locale} ${url} ${where}: columns for ${r.room.toFixed(1)}rem`,
              ).toBe(columnsFor(r.room));
              seenColumns.add(r.columns);
            };
            for (const width of widths) {
              await page.setViewportSize({ width, height: 900 });
              await check(page, `at ${width}px`);
            }
            // A text size of 150 and 200 percent, the browser's own setting: the strip asks for more
            // room and stacks.
            for (const [width, percent] of large) {
              const big = sizes.get(percent)!.page;
              await big.setViewportSize({ width, height: 900 });
              await big.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
              await check(big, `at ${width}px, ${percent}% text`);
            }
          }
          // Not vacuous: every maturity word a page shows in the language (stable for the six built-in
          // apps, beta and experimental for the bundled packages), and the line for an app that needs
          // no other app, was on a page that was measured.
          expect([...maturities].sort(), `${locale} maturity words measured`).toEqual(
            Object.values(messages[locale].apps.meta.maturities).sort(),
          );
          expect(standalone, `${locale} pages with no other app measured`).toBeGreaterThan(0);
        }
        // Every layout the strip has was seen: one column, two by two and one row of four.
        expect([...seenColumns].sort(), "strip layouts measured").toEqual([1, 2, 4]);
      } finally {
        for (const { context } of sizes.values()) await context.close();
      }
    });

    test("the cards keep every word that fits whole, and nothing leaves its card", async ({
      page,
      request,
    }) => {
      // The limit, stated: a word wider than its card (Swedish "Integritetsförhandsgranskning" in a
      // card 15 rem wide) cannot be kept whole by any rule, so it breaks at the card's edge; the
      // browser hyphenates it only where it has a dictionary (hyphens: auto), and it is switched off
      // here so that the result is the same on every browser. Every other word stays whole, and no
      // text leaves its card.
      let wide = 0;
      let cards = 0;
      for (const locale of ["de", "sv", "fi", "pl"] as const) {
        const urls = (await pagesOf(request, locale)).filter((u) => /\/apps\/.+\/$/.test(u));
        for (const url of urls) {
          await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
          await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
          await page.evaluate(() =>
            document
              .querySelectorAll("main details")
              .forEach((d) => ((d as HTMLDetailsElement).open = true)),
          );
          for (const width of [320, 375, 768, 1024, 1280]) {
            await page.setViewportSize({ width, height: 900 });
            const r = await brokenWords(page, ".app-card h3, .app-card-text", true);
            expect(r.cut, `${url} at ${width}px`).toEqual([]);
            wide += r.wide;
            const out = await page.evaluate(() =>
              [...document.querySelectorAll(".app-card")]
                .filter((c) => c.scrollWidth > c.clientWidth + 1)
                .map((c) => (c.querySelector("h3")?.textContent ?? "").trim().slice(0, 30)),
            );
            expect(out, `${url} at ${width}px: text leaves its card`).toEqual([]);
            cards += await page.locator(".app-card").count();
          }
        }
      }
      // Not vacuous: there are cards, and some word really is wider than its card.
      expect(cards).toBeGreaterThan(1000);
      expect(wide, "words wider than their card, which the test allows to break").toBeGreaterThan(
        0,
      );
    });

    test("the AI list, the card titles and the card text keep every word that fits whole at a large text size on a phone", async ({
      browser,
      request,
    }) => {
      // The lines about AI agents sat beside an icon, and the box round them was in rem: at 200
      // percent text on a 320 px phone the line had 158 of the box's 256 px, and English words that fit
      // the box ("placeholders", "permissions") were cut inside. A card
      // title ("statement") was cut the same way. Hyphenation is switched off (English has none anyway,
      // so what is measured is what a visitor sees), every fold is open, and a word wider than the
      // whole line may still break: nothing can keep it whole.
      let items = 0;
      let wide = 0;
      for (const [width, percent] of [
        [320, 200],
        [375, 200],
        [320, 150],
        [320, 125],
      ] as const) {
        const { page, context } = await pageWithTextSize(browser, percent, { width, height: 900 });
        try {
          for (const url of (await pagesOf(request, "en")).filter((u) => /\/apps\/.+\/$/.test(u))) {
            await page.goto(`${base()}${url}`, { waitUntil: "domcontentloaded" });
            await page.addStyleTag({ content: "* { hyphens: manual !important; }" });
            await page.evaluate(() =>
              document
                .querySelectorAll("main details")
                .forEach((d) => ((d as HTMLDetailsElement).open = true)),
            );
            const r = await brokenWords(page, ".app-safe li, .app-card h3, .app-card-text", true);
            expect(r.cut, `${url} at ${width}px with ${percent}% text`).toEqual([]);
            wide += r.wide;
            items += await page.locator(".app-safe li, .app-card").count();
          }
        } finally {
          await context.close();
        }
      }
      // Not vacuous: there are lines and cards in numbers, and some word really is wider than its line.
      expect(items).toBeGreaterThan(1000);
      expect(wide, "words wider than their line, which the test allows to break").toBeGreaterThan(
        0,
      );
    });

    test("the Markdown copy of every app page says what the page says, in every language", async ({
      page,
      request,
    }) => {
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
              // The caveats under the first heading of the folded part ("What the package does and
              // does not do"): the list items between it and the next heading.
              limits: (() => {
                const out: string[] = [];
                const first = document.querySelector(".app-dev .prose h3");
                for (
                  let el = first?.nextElementSibling ?? null;
                  el && el.tagName !== "H3";
                  el = el.nextElementSibling
                )
                  out.push(...[...el.querySelectorAll("li")].map((li) => t(li)));
                return out;
              })(),
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
          for (let i = first; first >= 0 && lines[i]?.startsWith("- **"); i++)
            strip.push(lines[i]!);
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
          // The caveats the plain text leaves out are the same list on the page and in the copy, item
          // for item (an app without any has none in both).
          const pkg = lines.indexOf(`### ${messages[locale].apps.h.package}`);
          expect(pkg, `${where} package heading`).toBeGreaterThan(0);
          const next = lines.findIndex((l, i) => i > pkg && l.startsWith("### "));
          expect(
            lines
              .slice(pkg + 1, next)
              .filter((l) => l.startsWith("- "))
              .map((l) => l.slice(2).replace(/\*\*/g, "")),
            `${where} caveats under the package text`,
          ).toEqual(view.limits);
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

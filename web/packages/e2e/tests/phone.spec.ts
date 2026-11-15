import { writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, open, test } from "../support/fixtures.ts";

// Every screen of the back office at phone widths. Each screen is measured, not looked at: nothing
// may stand past the edge of the screen, the page may not scroll sideways, and no box may be scrolled
// sideways on its own (a box that is, shows its content cut at the start, as the Security tab did).
// A few boxes scroll sideways on purpose (the kanban board, wide tables, the strip of tabs, the sort
// strip of a list); they are listed in SCROLLERS and may hold content past the edge, but they must
// start at their beginning, except a strip of tabs, which scrolls its selected tab into view: that tab
// must then be wholly inside the strip.
//
// OBOP_PHONE_REPORT=FILE writes every finding to FILE and fails nothing (a survey); OBOP_PHONE_SHOTS=DIR
// saves a screenshot of every screen for a visual review.

const WIDTHS = [320, 375];
const REPORT = process.env.OBOP_PHONE_REPORT;
const SHOTS = process.env.OBOP_PHONE_SHOTS;

/** Boxes that scroll sideways by design. */
const SCROLLERS = [
  ".kanban",
  ".table-wrap",
  ".tablewrap",
  ".tabs",
  ".pivot-wrap",
  ".cal-scroll",
  ".list > thead > tr",
];

type Finding = {
  pageOverflow: number;
  offscreen: { sel: string; left: number; right: number }[];
  scrollers: { sel: string; extra: number; left: number; allowed: boolean }[];
  clipped: { sel: string; text: string }[];
  smallTargets: { sel: string; w: number; h: number }[];
  /** Strips of tabs whose selected tab is not wholly inside the strip. */
  hiddenTabs: { sel: string; tab: string }[];
};

async function measure(page: Page): Promise<Finding> {
  return page.evaluate((allowedSelectors) => {
    const vw = document.documentElement.clientWidth;
    const path = (el: Element): string => {
      const parts: string[] = [];
      for (
        let e: Element | null = el;
        e && e !== document.body && parts.length < 4;
        e = e.parentElement
      ) {
        const cls = [...e.classList].slice(0, 2).join(".");
        parts.unshift(`${e.tagName.toLowerCase()}${e.id ? `#${e.id}` : ""}${cls ? `.${cls}` : ""}`);
      }
      return parts.join(" > ");
    };
    const hidden = (el: Element) => {
      for (let e: Element | null = el; e; e = e.parentElement) {
        if (e.classList.contains("sr-only")) return true;
        const s = getComputedStyle(e);
        if (s.display === "none" || s.visibility === "hidden") return true;
        // The visually hidden pattern (a header row of a stacked table, a label kept for readers).
        if (s.clipPath === "inset(50%)") return true;
      }
      return false;
    };
    const scrollsX = (s: CSSStyleDeclaration) => s.overflowX === "auto" || s.overflowX === "scroll";
    const out: Finding = {
      pageOverflow: Math.max(0, document.documentElement.scrollWidth - vw),
      offscreen: [],
      scrollers: [],
      clipped: [],
      smallTargets: [],
      hiddenTabs: [],
    };
    for (const strip of document.querySelectorAll(".tabs")) {
      const on = strip.querySelector('[aria-selected="true"]');
      if (!on || hidden(strip)) continue;
      const s = strip.getBoundingClientRect();
      const b = on.getBoundingClientRect();
      if (b.left < s.left - 1 || b.right > s.right + 1)
        out.hiddenTabs.push({ sel: path(strip), tab: (on.textContent ?? "").trim() });
    }
    const reported = new Set<Element>();
    for (const el of document.body.querySelectorAll("*")) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      if (hidden(el)) continue;
      const s = getComputedStyle(el);
      if (scrollsX(s) && el.scrollWidth > el.clientWidth + 1) {
        out.scrollers.push({
          sel: path(el),
          extra: el.scrollWidth - el.clientWidth,
          left: Math.round(el.scrollLeft),
          allowed: allowedSelectors.some((a) => el.matches(a)),
        });
      }
      if (r.right > vw + 1 || r.left < -1) {
        // Past the screen's edge. Inside a box that scrolls sideways it can be scrolled to; anywhere
        // else it cannot be seen. Only the outermost such element is reported.
        let scroller: Element | null = null;
        for (let p = el.parentElement; p; p = p.parentElement)
          if (scrollsX(getComputedStyle(p)) && p.scrollWidth > p.clientWidth + 1) {
            scroller = p;
            break;
          }
        let parentReported = false;
        for (let p = el.parentElement; p; p = p.parentElement)
          if (reported.has(p)) parentReported = true;
        // Past the right edge counts only when the nearest box that clips (or the page) overflows:
        // spaces at the end of a wrapped line hang past it without making anything wider.
        let clipper: Element = document.documentElement;
        for (let p = el.parentElement; p; p = p.parentElement)
          if (getComputedStyle(p).overflowX !== "visible") {
            clipper = p;
            break;
          }
        const overflows = r.left < -1 || clipper.scrollWidth > clipper.clientWidth + 1;
        if (!scroller && !parentReported && overflows) {
          reported.add(el);
          out.offscreen.push({
            sel: path(el),
            left: Math.round(r.left),
            right: Math.round(r.right),
          });
        }
      }
      // Words cut off without an ellipsis by a box that hides what overflows it.
      if (
        (s.overflowX === "hidden" || s.overflowX === "clip") &&
        s.textOverflow !== "ellipsis" &&
        el.scrollWidth > el.clientWidth + 1 &&
        (el.textContent ?? "").trim() &&
        !el.closest(".sr-only")
      )
        out.clipped.push({ sel: path(el), text: (el.textContent ?? "").trim().slice(0, 40) });
      // A control a finger can hardly hit (WCAG 2.2 AA asks 24 by 24 CSS pixels).
      if (
        el.matches("button, a[href], input, select, [role=button], [role=tab]") &&
        (r.width < 24 || r.height < 24)
      )
        out.smallTargets.push({ sel: path(el), w: Math.round(r.width), h: Math.round(r.height) });
    }
    return out;
  }, SCROLLERS);
}

const report: Record<string, Finding> = {};

async function check(page: Page, name: string, width: number | string) {
  // Let entry animations (a panel sliding in, a dialog rising) end before measuring.
  await page.waitForTimeout(400);
  const f = await measure(page);
  if (SHOTS) {
    mkdirSync(SHOTS, { recursive: true });
    // The whole page when the page itself scrolls (as it does on a phone), the screen otherwise.
    // (A page taller than about twenty screens, the Guide, is pictured by its first screens only.)
    const tall = await page.evaluate(() => document.documentElement.scrollHeight > 16000);
    await page.screenshot({
      path: join(SHOTS, `${width}-${name}.png`),
      fullPage: true,
      ...(tall ? { clip: { x: 0, y: 0, width: Number(width) || 390, height: 8000 } } : {}),
    });
    // A long screen scrolls inside its own box (the page itself does not): one more picture per
    // screenful, so a review sees all of it. The box is put back at its top afterwards.
    const steps = await page.evaluate(() => {
      const boxes = [...document.querySelectorAll<HTMLElement>("*")].filter((el) => {
        const s = getComputedStyle(el);
        return (
          (s.overflowY === "auto" || s.overflowY === "scroll") &&
          el.scrollHeight > el.clientHeight + 40
        );
      });
      const box = boxes.sort(
        (a, b) => b.clientHeight * b.clientWidth - a.clientHeight * a.clientWidth,
      )[0];
      if (!box) return 0;
      box.dataset.phoneShot = "1";
      return Math.min(5, Math.ceil(box.scrollHeight / box.clientHeight) - 1);
    });
    for (let i = 1; i <= steps; i++) {
      await page.evaluate((n) => {
        const box = document.querySelector<HTMLElement>("[data-phone-shot]")!;
        box.scrollTop = n * (box.clientHeight - 60);
      }, i);
      await page.waitForTimeout(150);
      await page.screenshot({ path: join(SHOTS, `${width}-${name}-${i}.png`) });
    }
    await page.evaluate(() => {
      const box = document.querySelector<HTMLElement>("[data-phone-shot]");
      if (box) {
        box.scrollTop = 0;
        delete box.dataset.phoneShot;
      }
    });
  }
  if (REPORT) {
    report[`${width} ${name}`] = f;
    return;
  }
  expect(f.pageOverflow, `${name} at ${width}px: the page scrolls sideways`).toBe(0);
  expect(f.offscreen, `${name} at ${width}px: past the edge of the screen`).toEqual([]);
  expect(
    f.scrollers.filter((s) => !s.allowed),
    `${name} at ${width}px: a box scrolls sideways`,
  ).toEqual([]);
  expect(
    f.scrollers.filter((s) => s.left > 0 && !s.sel.endsWith(".tabs")),
    `${name} at ${width}px: a box is scrolled sideways, so its start is cut off`,
  ).toEqual([]);
  expect(f.hiddenTabs, `${name} at ${width}px: the selected tab is out of sight`).toEqual([]);
  expect(f.clipped, `${name} at ${width}px: words cut off`).toEqual([]);
}

/** The runs: two phone widths with a mouse, and a touch phone (the rules for a coarse pointer). */
const RUNS = [...WIDTHS.map((width) => ({ width, touch: false })), { width: 390, touch: true }];

test.describe("phone widths", () => {
  test.afterAll(() => {
    if (REPORT) writeFileSync(REPORT, JSON.stringify(report, null, 1));
  });

  for (const { width, touch } of RUNS) {
    const label = touch ? `${width}-touch` : String(width);
    test.describe(touch ? "on a touch screen" : "with a mouse", () => {
      if (touch) test.use({ hasTouch: true, isMobile: true });
      test(`every screen fits a ${label} px phone`, async ({
        page,
        signIn,
        api,
        uid,
        scratchModule,
      }) => {
        test.setTimeout(240_000);
        await page.setViewportSize({ width, height: 780 });

        await page.goto("/");
        await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
        await check(page, "sign-in", label);

        const model = await scratchModule({
          title: "Phone data",
          fields: [
            { name: "title", required: true },
            { name: "state", kind: "select", options: ["new", "qualified", "won", "lost"] },
            { name: "amount_minor", kind: "money" },
            { name: "due", kind: "date" },
            { name: "notes", kind: "longtext" },
          ],
        });
        for (let i = 0; i < 6; i++)
          await api.create("root", model, {
            title: uid(`A rather long record title number ${i}`),
            state: ["new", "qualified", "won", "lost"][i % 4],
            amount_minor: 125000 * (i + 1),
            due: `2026-11-${String(10 + i).padStart(2, "0")}`,
            notes: "Some notes.",
          });

        await signIn("root");
        await check(page, "home", label);

        await page.getByRole("button", { name: "Open the app launcher" }).click();
        await expect(page.getByRole("dialog", { name: "Apps" })).toBeVisible();
        await check(page, "launcher", label);
        await page.keyboard.press("Escape");

        await page.getByRole("button", { name: "Search apps, records and actions" }).click();
        await expect(page.getByRole("dialog", { name: "Command palette" })).toBeVisible();
        await check(page, "palette", label);
        await page.keyboard.press("Escape");

        for (const view of ["list", "kanban", "calendar", "pivot", "graph"]) {
          await open(page, `/${model}/${model}?view=${view}`);
          await expect(page.locator(`.content.view-${view}`)).toBeVisible();
          await expect(page.locator(".content .skeleton")).toHaveCount(0);
          await check(page, `view-${view}`, label);
        }

        await open(page, `/${model}/${model}?view=list`);
        await page.getByRole("button", { name: "Filters, group by and favorites" }).click();
        await check(page, "filters", label);
        await page.keyboard.press("Escape");

        const rows = await api.list("root", model);
        await open(page, `/${model}/${model}?view=list&id=${rows[0].id}`);
        await expect(page.getByRole("button", { name: "Save" })).toBeVisible();
        await check(page, "record", label);
        // On a phone the record is a sheet (a dialog), so it is found by its class.
        await page.locator("aside.record").evaluate((el) => el.scrollTo(0, el.scrollHeight));
        await check(page, "record-end", label);

        await open(page, `/${model}/${model}?view=list&id=new`);
        await expect(page.getByRole("button", { name: "Create" })).toBeVisible();
        await check(page, "record-new", label);

        // On a phone Import and Export sit in the ⋯ menu of the title row.
        await open(page, `/${model}/${model}?view=list`);
        await page.getByRole("button", { name: "More actions" }).click();
        await check(page, "more-menu", label);
        await page.getByRole("menuitem", { name: "Import from a CSV file" }).click();
        await expect(
          page.getByRole("dialog", { name: /^Import .* from a CSV file$/ }),
        ).toBeVisible();
        await check(page, "import", label);
        await page.keyboard.press("Escape");

        for (const tab of [
          "theme",
          "apps",
          "identity",
          "modules",
          "updates",
          "build",
          "security",
          "brand",
          "guide",
        ]) {
          await open(page, `/customize/${tab}?tab=${tab}`);
          // The app shows "Loading" while it fetches what a tab needs; on a busy machine that can
          // take longer than an assertion's default wait.
          await expect(page.getByRole("region", { name: "Loading" })).toHaveCount(0, {
            timeout: 20_000,
          });
          await expect(page.getByRole("tab", { name: /./, selected: true }).first()).toBeVisible();
          await expect(page.locator(".content .skeleton")).toHaveCount(0);
          await check(page, `customize-${tab}`, label);
        }

        await open(page, "/customize/modules?tab=modules");
        await page.locator("article.card").first().click();
        await check(page, "module-detail", label);

        await open(page, "/connectors");
        await check(page, "connectors", label);
      });
    });
  }
});

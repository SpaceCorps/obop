import type { Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";

// The pivot and the graph open on a measure that means something, keep currencies apart, and draw
// their text at a size a person can read (UI plan 2.5, bug B11).

const fields = [
  { name: "title", required: true },
  { name: "state", kind: "select" as const, options: ["draft", "confirmed", "cancelled"] },
  // The first money field is a unit price: averaged, never summed, so it is not the default.
  { name: "unit_price_minor", kind: "money" as const },
  { name: "amount_minor", kind: "money" as const },
  { name: "last_seq", kind: "number" as const },
  { name: "currency", kind: "select" as const, options: ["EUR", "SEK"] },
  { name: "due", kind: "date" as const },
];

const data = [
  ["draft", 1000, 10000, "EUR"],
  ["draft", 2000, 5000, "EUR"],
  ["confirmed", 1500, 20000, "EUR"],
  ["confirmed", 900, 70000, "SEK"],
  ["cancelled", 500, 1000, "EUR"],
] as const;

async function seeded(
  api: Api,
  scratchModule: (s: any) => Promise<string>,
  uid: (p: string) => string,
) {
  const model = await scratchModule({ title: "Chart data", fields });
  for (const [state, unit, amount, currency] of data)
    await api.create("root", model, {
      title: uid("Row"),
      state,
      unit_price_minor: unit,
      amount_minor: amount,
      currency,
      last_seq: 7,
      due: "2026-10-15",
    });
  return model;
}

/** The smallest rendered height of a chart's axis texts, and how much the drawing is scaled. */
async function axisText(page: Page, root: string) {
  return page.locator(root).evaluate((el) => {
    const svg = el.querySelector("svg.chartsvg, svg.piesvg") as SVGSVGElement | null;
    if (!svg) return null;
    const texts = [...svg.querySelectorAll("text.axis")] as SVGTextElement[];
    const shown = texts.filter((x) => x.textContent?.trim());
    return {
      count: shown.length,
      minHeight: Math.min(...shown.map((x) => x.getBoundingClientRect().height)),
      minFont: Math.min(...shown.map((x) => parseFloat(getComputedStyle(x).fontSize))),
      scale: svg.getBoundingClientRect().width / svg.viewBox.baseVal.width,
    };
  });
}

test.describe("charts", () => {
  test("the pivot opens on a money sum, offers no sum of a counter, and keeps currencies apart", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await seeded(api, scratchModule, uid);
    await signIn("root", `/${model}/${model}?view=pivot`);
    const pivot = page.locator(".pivot");
    const measure = pivot.getByLabel("Measure");

    // A summable money field, not the unit price that comes first, and not the count.
    await expect(measure).toHaveValue("amount_minor");
    const offered = await measure.locator("option").allTextContents();
    expect(offered).toContain("Sum of Amount");
    expect(offered).toContain("Average of Unit price");
    expect(offered.some((o) => /Last seq/.test(o))).toBe(false);
    expect(offered.some((o) => /Sum of Unit price/.test(o))).toBe(false);

    // EUR and SEK are columns of their own, never one sum (B11), so there is no warning.
    await pivot.getByLabel("Rows").selectOption("state");
    const table = page.getByRole("table");
    await expect(table.getByRole("columnheader", { name: "EUR" })).toBeVisible();
    await expect(table.getByRole("columnheader", { name: "SEK" })).toBeVisible();
    const confirmed = table.getByRole("row", { name: /^Confirmed/ });
    await expect(confirmed).toContainText("€200.00");
    await expect(confirmed).toContainText("SEK");
    await expect(confirmed).toContainText("700.00");
    await expect(table.locator("tfoot")).toContainText("€360.00");
    await expect(page.locator(".notice-bar")).toHaveCount(0);

    // A count has no currency: one column, the record count, and no currency notice either.
    await measure.selectOption("count");
    await expect(table.getByRole("columnheader", { name: "SEK" })).toHaveCount(0);
    await expect(table.locator("tfoot")).toContainText("5");
    await expect(page.locator(".notice-bar")).toHaveCount(0);

    // The choice is kept in the address, so a shared link opens the same table.
    await expect(page).toHaveURL(/count/);
  });

  test("the graph is drawn at the size of its container, with axis text of at least 11 px", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await seeded(api, scratchModule, uid);
    await signIn("root", `/${model}/${model}?view=graph`);
    const graph = page.locator(".graph");
    await graph.getByLabel("Dimension").selectOption("state");
    await expect(graph.locator("svg.chartsvg")).toBeVisible();

    for (const width of [1440, 834, 390, 320]) {
      await page.setViewportSize({ width, height: 900 });
      await expect
        .poll(async () => (await axisText(page, ".graph figure.chart"))?.scale ?? 0)
        .toBeCloseTo(1, 1);
      const m = (await axisText(page, ".graph figure.chart"))!;
      expect(m.count, `axis labels at ${width} px`).toBeGreaterThan(0);
      expect(m.minFont, `font size at ${width} px`).toBeGreaterThanOrEqual(11);
      expect(m.minHeight, `rendered text height at ${width} px`).toBeGreaterThanOrEqual(11);
      // Nothing makes the page scroll sideways.
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      );
      expect(overflow, `sideways scroll at ${width} px`).toBeLessThanOrEqual(0);
    }

    // A status is coloured by the tones of its badges: confirmed reads as done, cancelled as bad.
    await page.setViewportSize({ width: 1280, height: 900 });
    await expect(graph.locator("g.mark.t-ok")).toHaveCount(1);
    await expect(graph.locator("g.mark.t-bad")).toHaveCount(1);
  });

  test("more than ten groups turn into sorted horizontal bars with the rest as Other", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Many groups",
      fields: [
        { name: "title", required: true },
        {
          name: "kind",
          kind: "select",
          options: Array.from({ length: 14 }, (_, i) => `k${i}`),
        },
        { name: "amount_minor", kind: "money" },
      ],
    });
    for (let i = 0; i < 14; i++)
      await api.create("root", model, {
        title: uid("Row"),
        kind: `k${i}`,
        amount_minor: (i + 1) * 100,
      });
    await signIn("root", `/${model}/${model}?view=graph`);
    const graph = page.locator(".graph");
    await graph.getByLabel("Dimension").selectOption("kind");
    await expect(graph.locator("svg.chartsvg.horizontal")).toBeVisible();
    const marks = graph.locator("g.mark");
    await expect(marks).toHaveCount(9);
    // The largest first, the smallest six added up as Other: 1 + 2 + ... + 6 = 21 (€21.00).
    await expect(marks.first()).toHaveAttribute("aria-label", /^K13: €14\.00/i);
    await expect(marks.last()).toHaveAttribute("aria-label", /^Other \(6\): €21\.00/);
    await expect(graph.getByText("The 8 largest are shown")).toBeVisible();
    // Other opens the list without the groups the chart shows.
    await marks.last().click();
    await expect(page).toHaveURL(/view=list/);
    await expect(page.locator(".controlpanel .count")).toHaveText("6 records");
  });
});

import { expect, test, type Api } from "../support/fixtures.ts";
import type { Page } from "@playwright/test";

// Print a quotation from where a record opens by default: the side panel on a desktop and the
// full-screen sheet on a phone (UI plan 3.3). On paper the document is the only thing, once, across
// the full page width, and dark on white even when the screen is dark.

/** The number of pages of a PDF made by Chrome. */
function pages(pdf: Buffer): number {
  return (pdf.toString("latin1").match(/\/Type\s*\/Page[^s]/g) ?? []).length;
}

/** What print media shows: the boxes left in the layout, the document's box and its text colour. */
async function onPaper(page: Page) {
  await page.emulateMedia({ media: "print" });
  const out = await page.evaluate(() => {
    const shown = [...document.body.children].filter(
      (el) => getComputedStyle(el).display !== "none",
    );
    const doc = document.querySelector(".print-sheet .so-print") as HTMLElement | null;
    const r = doc?.getBoundingClientRect();
    // The rendered colour as sRGB channels (a canvas resolves oklch and system colours alike).
    const c = document.createElement("canvas").getContext("2d")!;
    c.fillStyle = doc ? getComputedStyle(doc).color : "transparent";
    c.fillRect(0, 0, 1, 1);
    const [red, green, blue] = c.getImageData(0, 0, 1, 1).data;
    return {
      shown: shown.map((el) => el.className),
      x: r?.x ?? -1,
      width: r?.width ?? 0,
      viewport: innerWidth,
      text: doc?.innerText ?? "",
      brightness: ((red ?? 0) + (green ?? 0) + (blue ?? 0)) / 3,
    };
  });
  await page.emulateMedia({ media: null });
  return out;
}

async function aQuotation(api: Api, uid: (p: string) => string) {
  const customer = await api.create("root", "party", { name: uid("Print customer") });
  const order = await api.create("root", "sale_order", { party_id: customer.id });
  await api.create("root", "sale_order_line", {
    order_id: order.id,
    description: uid("Torque wrench set"),
    quantity: 2,
    unit_price_minor: 12900,
  });
  return { customer, order };
}

test.describe("sales print", () => {
  test("from the side panel on a desktop: one full-width page", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const { customer, order } = await aQuotation(api, uid);
    await signIn("root", `/sales/sale_order?id=${order.id}`);
    await expect(page.getByRole("button", { name: "Print" })).toBeVisible();

    const paper = await onPaper(page);
    expect(paper.shown).toEqual(["print-sheet"]);
    expect(paper.x).toBe(0);
    expect(paper.width).toBe(paper.viewport);
    expect(paper.text).toContain(customer.name);
    expect(paper.text).toContain("Torque wrench set");
    expect(paper.brightness).toBeLessThan(80);
    expect(pages(await page.pdf({ format: "A4" }))).toBe(1);
  });

  test("from the phone sheet in dark mode: dark text on one page", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: "dark" });
    const { customer, order } = await aQuotation(api, uid);
    await signIn("root", `/sales/sale_order?id=${order.id}`);
    await expect(page.getByRole("button", { name: "Print" })).toBeVisible();

    const paper = await onPaper(page);
    expect(paper.shown).toEqual(["print-sheet"]);
    expect(paper.text).toContain(customer.name);
    expect(paper.brightness).toBeLessThan(80);
    expect(pages(await page.pdf({ format: "A4" }))).toBe(1);
  });
});

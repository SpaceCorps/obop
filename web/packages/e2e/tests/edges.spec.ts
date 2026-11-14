import { expect, test } from "../support/fixtures.ts";

// What sits at an edge of the screen or of a scrolling box: the bar of a record panel, the head of a
// scrolled kanban column, a tooltip in a corner and the search of the top bar on a narrow screen.
// Each test measures the layout, so a regression shows as numbers, not as a picture to compare.

const many = Array.from({ length: 16 }, (_, i) => ({ name: `note_${i + 1}` }));

test.describe("edges of the screen", () => {
  test("the bar of a long record stays whole on the bottom edge of the panel while the fields scroll", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Long form",
      fields: [{ name: "title", required: true }, ...many],
    });
    const r = await api.create("root", model, { title: uid("Long") });
    await signIn("root", `/${model}/${model}?view=list&id=${r.id}`);
    const panel = page.getByRole("complementary", { name: /record$/ });
    const save = panel.getByRole("button", { name: "Save" });
    await expect(save).toBeVisible();

    const gaps = await panel.evaluate((el) => {
      const bar = el.querySelector(".formbar")!.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      const btn = el.querySelector(".formbar button")!.getBoundingClientRect();
      const inner = box.bottom - parseFloat(getComputedStyle(el).borderBottomWidth);
      return {
        scrolls: el.scrollHeight > el.clientHeight,
        barToEdge: Math.abs(inner - bar.bottom),
        buttonInside: btn.bottom <= inner,
      };
    });
    expect(gaps.scrolls, "the record is long enough to scroll").toBe(true);
    // The bar used to stick 1.25rem below the edge, which cut off the lower part of Save and Close.
    expect(gaps.barToEdge).toBeLessThan(1.5);
    expect(gaps.buttonInside).toBe(true);
  });

  test("the bar of a short record sits on the bottom edge of the panel, not under the last field", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Short form",
      fields: [{ name: "title", required: true }],
    });
    const r = await api.create("root", model, { title: uid("Short") });
    await signIn("root", `/${model}/${model}?view=list&id=${r.id}`);
    const panel = page.getByRole("complementary", { name: /record$/ });
    await expect(panel.getByRole("button", { name: "Save" })).toBeVisible();
    const gap = await panel.evaluate((el) => {
      const bar = el.querySelector(".formbar")!.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      return box.bottom - parseFloat(getComputedStyle(el).borderBottomWidth) - bar.bottom;
    });
    expect(Math.abs(gap)).toBeLessThan(1.5);
  });

  test("the head of a scrolled kanban column covers the cards that pass under it", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Scroll board",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select" as const, options: ["new", "won"] },
      ],
    });
    for (let i = 0; i < 25; i++)
      await api.create("root", model, { title: uid("Card"), state: "won" });
    await signIn("root", `/${model}/${model}?view=kanban`);
    const col = page.getByRole("region", { name: /^Won, 25 cards$/i });
    await expect(col.locator(".kcard")).toHaveCount(25);

    const seen = await col.evaluate(async (el) => {
      el.scrollTop = 300;
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const head = el.querySelector("header")!;
      const h = head.getBoundingClientRect();
      const c = el.getBoundingClientRect();
      const top = c.top + parseFloat(getComputedStyle(el).borderTopWidth);
      // What is drawn just inside the column's top edge and in the empty right part of the head.
      const atTop = document.elementFromPoint(c.left + c.width / 2, top + 1);
      const atEnd = document.elementFromPoint(h.right - 3, h.top + h.height / 2);
      return {
        scrolled: el.scrollTop > 0,
        headAtTop: Math.abs(h.top - top),
        topIsHead: !!atTop?.closest("header"),
        endIsHead: !!atEnd?.closest("header"),
        opaque: getComputedStyle(head).backgroundColor !== "rgba(0, 0, 0, 0)",
      };
    });
    expect(seen.scrolled).toBe(true);
    // The head stopped 0.5rem down, with no background, so the cards showed through above and behind it.
    expect(seen.headAtTop).toBeLessThan(1.5);
    expect(seen.topIsHead, "nothing of a card shows above the head").toBe(true);
    expect(seen.endIsHead, "the head stays above the cards that pass under it").toBe(true);
    expect(seen.opaque).toBe(true);
  });

  test("a tooltip of a button in the top-left corner stays inside the window", async ({
    page,
    signIn,
  }) => {
    await signIn("root");
    const launcher = page.getByRole("button", { name: "Open the app launcher" });
    await launcher.hover();
    const tip = page.getByRole("tooltip", { name: "Open the app launcher" });
    await expect(tip).toBeVisible();
    const box = (await tip.boundingBox())!;
    const width = page.viewportSize()!.width;
    // Centred under the button it began 55 px left of the window and its first words were cut off.
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(width);
  });

  test("the search of the top bar is a field with words on a tablet and an icon button on a phone", async ({
    page,
    signIn,
  }) => {
    await page.setViewportSize({ width: 800, height: 900 });
    await signIn("root");
    const search = page.getByRole("button", { name: "Search apps, records and actions" });
    await expect(search).toBeVisible();
    await expect(search.getByText("Search apps, records and actions")).toBeVisible();

    await page.setViewportSize({ width: 375, height: 800 });
    await expect(search).toBeVisible();
    await expect(search.getByText("Search apps, records and actions")).toBeHidden();
    await expect(search.locator(".kbd")).toBeHidden();
    // An icon button beside the theme and account buttons, not a framed box in the middle of the bar.
    const look = await search.evaluate((el) => {
      const s = getComputedStyle(el);
      const tray = el.nextElementSibling!.getBoundingClientRect();
      return { border: s.borderTopWidth, gap: tray.left - el.getBoundingClientRect().right };
    });
    expect(look.border).toBe("0px");
    expect(look.gap).toBeLessThan(8);
    await search.click();
    await expect(page.getByRole("dialog", { name: "Command palette" })).toBeVisible();
  });
});

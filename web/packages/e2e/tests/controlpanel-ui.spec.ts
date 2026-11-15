import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// The model page's control panel (UI plan 2.9): the filter panel lives in the top layer, so a short
// list on a tablet cannot clip it (B2); New from a list opened on a parent fills the parent in
// (B5); the empty states say what to do next; typing filters at once; generated filters.

const panelButton = (page: Page) =>
  page.getByRole("button", { name: "Filters, group by and favorites" });
const count = (page: Page) => page.locator(".controlpanel .count");
const st = (q: unknown[]) => encodeURIComponent(JSON.stringify({ q }));

/** Whether the centre of the element is the element itself (not covered, not clipped away). */
const reachable = (page: Page, selector: string) =>
  page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return false;
    const r = el.getBoundingClientRect();
    const x = r.left + r.width / 2;
    const y = r.top + r.height / 2;
    if (y > innerHeight || y < 0) return false;
    const hit = document.elementFromPoint(x, y);
    return !!hit && (hit === el || el.contains(hit));
  }, selector);

test.describe("control panel", () => {
  test.describe("on a tablet", () => {
    test.use({ viewport: { width: 834, height: 1112 }, hasTouch: true });

    test("the filter panel is whole over a list of two rows", async ({
      page,
      signIn,
      api,
      scratchModule,
      uid,
    }) => {
      const model = await scratchModule({
        title: "Tablet panel",
        fields: [
          { name: "title", required: true },
          { name: "stage", kind: "select", options: ["new", "won", "lost"] },
          { name: "due_on", kind: "date" },
        ],
      });
      const tag = uid("Panel");
      for (const n of [1, 2])
        await api.create("root", model, { title: `${tag} ${n}`, stage: "new" });
      for (const n of [3, 4, 5])
        await api.create("root", model, { title: `Other ${n}`, stage: "won" });
      await signIn("root", `/${model}/${model}?view=list&st=${st([{ op: "search", v: tag }])}`);
      await expect(count(page)).toHaveText("2 records");

      await panelButton(page).click();
      const save = page.getByRole("button", { name: "Save current search" });
      await expect(save).toBeVisible();
      await save.scrollIntoViewIfNeeded();
      // The panel is not cut off by the short list: its last control can be pointed at and used.
      expect(await reachable(page, ".fp-wrap .fp-save button[type=submit]")).toBe(true);
      expect(await reachable(page, ".fp-wrap .fpanel")).toBe(true);
      await page.getByLabel("Name for this search").fill("Two rows");
      await save.click();
      await expect(page.getByRole("button", { name: "Two rows", exact: true })).toBeVisible();
      // No sideways scroll with the panel open.
      expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBe(0);
    });
  });

  test("New from a list opened on a parent fills the parent in, and the empty list offers it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const party = await api.create("root", "party", { name: uid("Context customer") });
    const order = await api.create("root", "sale_order", { party_id: party.id, currency: "EUR" });
    const facet = [{ op: "is", f: "order_id", v: [order.id] }];
    await signIn("root", `/sales/sale_order_line?view=list&st=${st(facet)}`);

    // No lines yet: the empty state names the order and offers to add the first line.
    const number = (await api.run("root", "sale_order.get", { id: order.id })).number as string;
    await expect(
      page.getByRole("heading", { name: `No order lines on ${number} yet` }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Add order line" }).click();

    const panel = page.getByRole("complementary", { name: "Order line record" });
    await expect(panel).toBeVisible();
    // The order is already chosen: the form links to it.
    await expect(panel.getByRole("link", { name: `Open ${number}` })).toBeVisible();
    await panel.getByLabel("Description").fill("Context line");
    await panel.getByLabel("Quantity").fill("2");
    await panel.getByLabel("Unit price").fill("10");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect
      .poll(async () =>
        (await api.list("root", "sale_order_line")).filter((l) => l.order_id === order.id),
      )
      .toHaveLength(1);

    // The page has heard of it: the form shows the saved line. Until then the form is still the
    // new one, and it closes only when its save is over (records.spec.ts).
    await expect(panel.getByRole("heading", { name: "Context line" })).toBeVisible();

    // The top New does the same once the list has a line.
    await page.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
    await page.getByRole("button", { name: "New", exact: true }).click();
    await expect(panel.getByRole("heading", { name: "New order line" })).toBeVisible();
    await expect(panel.getByRole("link", { name: `Open ${number}` })).toBeVisible();
  });

  test("an empty model offers New and Import; a search with no match offers Clear filters", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Empty start",
      fields: [{ name: "title", required: true }],
    });
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(page.getByRole("heading", { name: /^No .* yet$/ })).toBeVisible();
    await expect(page.getByRole("button", { name: "Import a CSV file" })).toBeVisible();
    await page.getByRole("button", { name: /^Create the first / }).click();
    const panel = page.locator("aside.record");
    await panel.getByLabel(/^Title/).fill("First one");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect.poll(async () => (await api.list("root", model)).length).toBe(1);

    await page.goto(
      `/#/${model}/${model}?view=list&st=${st([{ op: "search", v: "nothing like it" }])}`,
    );
    await expect(page.getByRole("heading", { name: "No matches" })).toBeVisible();
    await page.getByRole("button", { name: "Clear filters" }).click();
    await expect(count(page)).toHaveText("1 record");
  });

  test("typing filters at once, suggests the options, and Enter keeps it as a chip", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Live search",
      fields: [
        { name: "title", required: true },
        { name: "stage", kind: "select", options: ["new", "qualified", "won"] },
        { name: "iban", class: "financial" },
      ],
    });
    for (const [title, stage] of [
      ["Alpha", "new"],
      ["Alpine", "won"],
      ["Bravo", "qualified"],
    ])
      await api.create("root", model, { title, stage });
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(count(page)).toHaveText("3 records");

    const box = page.getByRole("combobox", { name: /^Search/ });
    await box.pressSequentially("alp");
    // No Enter: the list narrows while typing.
    await expect(count(page)).toHaveText("2 records");
    const options = page.getByRole("listbox", { name: "Search options" });
    await expect(options.getByRole("option", { name: "Search everywhere for: alp" })).toBeVisible();
    // A financial field (an IBAN) is never offered as a target.
    await expect(options.getByRole("option", { name: /iban/i })).toHaveCount(0);
    await box.press("Enter");
    await expect(page.locator(".facet")).toHaveText(/Search: alp/);
    await expect(box).toHaveValue("");
    await expect(count(page)).toHaveText("2 records");

    // An option's name is a suggestion of its own.
    await box.pressSequentially("qualif");
    await options.getByRole("option", { name: "Stage: Qualified" }).click();
    await expect(page.locator(".facet").last()).toHaveText(/Stage: Qualified/);
    await expect(count(page)).toHaveText("0 records");
  });

  test("generated filters: Open from the actions, a date range, Archived", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Generated",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "doing", "done"] },
        { name: "due_on", kind: "date" },
        { name: "active", kind: "bool" },
      ],
      actions: [
        { name: "start", field: "state", from: "new", to: "doing" },
        { name: "finish", field: "state", from: "doing", to: "done" },
      ],
    });
    const today = new Date().toISOString().slice(0, 10);
    const a = await api.create("root", model, { title: "One", due_on: today, active: true });
    await api.create("root", model, { title: "Two", due_on: "2020-01-01", active: false });
    await api.run("root", `${model}.start`, { id: a.id });
    await api.run("root", `${model}.finish`, { id: a.id });
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(count(page)).toHaveText("2 records");

    await panelButton(page).click();
    await page.getByRole("checkbox", { name: "Open", exact: true }).check();
    await expect(count(page)).toHaveText("1 record");
    await expect(page.locator(".facet")).toHaveText(/^Open/);
    await page.getByRole("checkbox", { name: "Open", exact: true }).uncheck();
    await expect(count(page)).toHaveText("2 records");

    await page.getByRole("button", { name: "Due on: Today", exact: true }).click();
    await expect(count(page)).toHaveText("1 record");
    await expect(page.locator(".facet")).toHaveText(/Due on: Today/);
    await page.getByRole("button", { name: "Due on: Today", exact: true }).click();

    await page.getByRole("checkbox", { name: "Archived", exact: true }).check();
    await expect(count(page)).toHaveText("1 record");
  });

  test.describe("on a phone", () => {
    test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

    test("one row of search, New floats, Import sits in the ⋯ menu", async ({
      page,
      signIn,
      api,
      scratchModule,
    }) => {
      const model = await scratchModule({
        title: "Phone bar",
        fields: [
          { name: "title", required: true },
          { name: "stage", kind: "select", options: ["new", "won"] },
        ],
      });
      await api.create("root", model, { title: "Only", stage: "new" });
      await signIn("root", `/${model}/${model}?view=list`);
      await expect(count(page)).toHaveText("1 record");
      const fab = page.locator(".cp-fab");
      await expect(fab).toBeVisible();
      await expect(page.getByRole("button", { name: "Import", exact: true })).toHaveCount(0);
      await page.getByRole("button", { name: "More actions" }).click();
      await page.getByRole("menuitem", { name: "Import from a CSV file" }).click();
      await expect(page.getByRole("dialog", { name: /^Import .* from a CSV file$/ })).toBeVisible();
      await page.keyboard.press("Escape");

      // The views are one labelled menu, and the filter sheet keeps Done in reach.
      await expect(page.getByLabel("View", { exact: true })).toHaveValue("list");
      await panelButton(page).click();
      await expect(page.getByRole("button", { name: "Done" })).toBeVisible();
      expect(await reachable(page, ".fp-head .btn")).toBe(true);
      await page.getByRole("button", { name: "Done" }).click();

      await fab.click();
      await expect(page.getByRole("button", { name: "Create" })).toBeVisible();
    });
  });
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { Page, Request } from "@playwright/test";
import { repoRoot } from "../support/env.ts";
import { expect, test, type Api } from "../support/fixtures.ts";

// D1 to D6 and the resilience work: select all matching, the record sheet on small screens, sorting
// with accented names, aggregated views that never load the whole table, a language switch that keeps
// the screen, and a friendly failure for a crash, a 500 and a lost connection.

const STAGES = ["new", "qualified", "won", "lost"];

async function fill(api: Api, model: string, n: number, make: (i: number) => object) {
  for (let from = 0; from < n; from += 25) {
    await Promise.all(
      Array.from({ length: Math.min(25, n - from) }, (_, k) =>
        api.create("root", model, make(from + k)),
      ),
    );
  }
}

const tr = (lang: string, id: string) => {
  const file = resolve(repoRoot, `web/apps/backoffice/locales/${lang}/messages.json`);
  const text = JSON.parse(readFileSync(file, "utf8"))[id];
  if (!text) throw new Error(`no ${lang} translation of ${JSON.stringify(id)}`);
  return text as string;
};

/** The JSON bodies of the `<model>/search` requests the page sends. */
function searches(page: Page, model: string) {
  const bodies: any[] = [];
  const on = (r: Request) => {
    if (r.method() === "POST" && r.url().endsWith(`/api/${model}/search`))
      bodies.push(JSON.parse(r.postData() ?? "{}"));
  };
  page.on("request", on);
  return bodies;
}

test.describe("select all matching", () => {
  test("sets a field on, exports and deletes 130 records across pages, with progress and a summary", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Bulk things",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: STAGES },
      ],
    });
    await fill(api, model, 130, (i) => ({
      title: `Thing ${String(i).padStart(3, "0")}`,
      state: "new",
    }));
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(page.getByRole("grid")).toBeVisible();

    // The page holds 100 of the 130 records.
    await page.getByRole("checkbox", { name: "Select all records" }).check();
    await expect(page.getByRole("toolbar", { name: "Bulk actions" })).toContainText("100 selected");
    await page.getByRole("button", { name: "Select all 130 matching" }).click();
    await expect(page.getByRole("toolbar", { name: "Bulk actions" })).toContainText("130 selected");
    await expect(page.getByText("All 130 matching records are selected.")).toBeVisible();

    // Export streams every matching record, not only the page on screen.
    const download = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export selected" }).click();
    const file = await (await download).path();
    const lines = readFileSync(file, "utf8").trim().split(/\r?\n/);
    expect(lines).toHaveLength(131); // the header and 130 records

    // Set a field on all of them.
    await page.getByRole("button", { name: "Set a field on the selected records" }).click();
    await page.getByRole("button", { name: "Won", exact: true }).click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toContainText("Set State to Won on 130 records that match this search?");
    await dialog.getByRole("button", { name: "Change 130 records" }).click();
    await expect(dialog.getByTestId("bulk-summary")).toHaveText(
      "130 done, 0 waiting for approval, 0 failed.",
    );
    await dialog.getByRole("button", { name: "Close", exact: true }).last().click();
    const rows = await api.list("root", model);
    expect(rows).toHaveLength(130);
    expect(rows.every((r) => r.state === "won")).toBe(true);

    // Delete all of them.
    await expect(page.getByRole("toolbar", { name: "Bulk actions" })).toHaveCount(0);
    await page.getByRole("checkbox", { name: "Select all records" }).check();
    await page.getByRole("button", { name: "Select all 130 matching" }).click();
    await page
      .getByRole("toolbar", { name: "Bulk actions" })
      .getByRole("button", { name: "Delete" })
      .click();
    await expect(dialog).toContainText(
      "Delete 130 records that match this search? This cannot be undone.",
    );
    await dialog.getByRole("button", { name: "Delete 130 records" }).click();
    await expect(dialog.getByTestId("bulk-summary")).toContainText("130 done");
    await dialog.getByRole("button", { name: "Close", exact: true }).last().click();
    expect(await api.list("root", model)).toHaveLength(0);
    await expect(page.getByText(/No .* yet/)).toBeVisible();
  });

  test("a filter limits what 'all matching' means", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Filtered bulk",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: STAGES },
      ],
    });
    await fill(api, model, 200, (i) => ({ title: `Row ${i}`, state: i % 4 === 0 ? "won" : "new" }));
    const st = JSON.stringify({ q: [{ op: "is", f: "state", v: ["new"] }] });
    await signIn("root", `/${model}/${model}?view=list&st=${encodeURIComponent(st)}`);
    await page.getByRole("checkbox", { name: "Select all records" }).check();
    await page.getByRole("button", { name: "Select all 150 matching" }).click();
    await page.getByRole("button", { name: "Delete" }).click();
    const dialog = page.getByRole("alertdialog");
    await dialog.getByRole("button", { name: "Delete 150 records" }).click();
    await expect(dialog.getByTestId("bulk-summary")).toContainText("150 done");
    await dialog.getByRole("button", { name: "Close", exact: true }).last().click();
    const left = await api.list("root", model);
    expect(left).toHaveLength(50);
    expect(left.every((r) => r.state === "won")).toBe(true);
  });
});

test.describe("the record opens as a sheet on small screens", () => {
  for (const [width, height, kind] of [
    [1024, 768, "tablet"],
    [768, 1024, "tablet"],
    [390, 844, "phone"],
  ] as const) {
    test(`${width} px: ${kind}`, async ({ page, signIn, api, scratchModule, uid }) => {
      const model = await scratchModule({
        title: "Sheet things",
        fields: [{ name: "title", required: true }],
      });
      const title = uid("Sheet");
      await fill(api, model, 30, (i) => ({ title: i === 0 ? title : `Filler ${i}` }));
      await page.setViewportSize({ width, height });
      await signIn("root", `/${model}/${model}`);
      // The first column opens the record.
      await page.getByRole("gridcell", { name: title }).first().click();
      const sheet = page.locator("aside.record");
      await expect(sheet).toBeVisible();
      await expect(sheet).toHaveAttribute("role", "dialog");
      await expect(page.locator("html")).toHaveClass(/sheet-open/);
      // It sits over the list at the inline end (a tablet) or covers the screen (a phone); the
      // slide-in animation has to finish first.
      await expect
        .poll(async () => {
          const b = (await sheet.boundingBox())!;
          return [Math.round(b.x + b.width), Math.round(b.y), Math.round(b.height)];
        })
        .toEqual([width, 0, height]);
      const box = (await sheet.boundingBox())!;
      if (kind === "phone") expect(Math.round(box.width)).toBe(width);
      else expect(box.width).toBeLessThan(width);
      // The list is still there underneath.
      await expect(page.getByRole("grid")).toBeAttached();
      // Focus stays inside the sheet.
      for (let i = 0; i < 6; i++) await page.keyboard.press("Tab");
      expect(await sheet.evaluate((el) => el.contains(document.activeElement))).toBe(true);
      if (kind === "phone")
        await expect(sheet.getByRole("button", { name: "Back to list" })).toBeVisible();
      // Escape closes it, the page scrolls again and the list is where it was.
      await page.keyboard.press("Escape");
      await expect(sheet).toHaveCount(0);
      await expect(page.locator("html")).not.toHaveClass(/sheet-open/);
      await expect(page.getByRole("grid")).toBeVisible();
    });
  }

  test("a phone's back button closes it, and the desktop panel is not a dialog", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Back things",
      fields: [{ name: "title", required: true }],
    });
    await api.create("root", model, { title: "Only one" });
    await page.setViewportSize({ width: 390, height: 844 });
    await signIn("root", `/${model}/${model}`);
    await page.getByRole("gridcell", { name: "Only one" }).first().click();
    await page.getByRole("button", { name: "Back to list" }).click();
    await expect(page.locator("aside.record")).toHaveCount(0);
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.getByRole("gridcell", { name: "Only one" }).first().click();
    await expect(page.locator("aside.record")).not.toHaveAttribute("role", "dialog");
    await expect(page.locator("html")).not.toHaveClass(/sheet-open/);
  });
});

test.describe("sorting with accented names", () => {
  test.use({ language: "sv" });
  test("the server and the browser order Swedish names the same way", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Names",
      fields: [{ name: "title", required: true }],
    });
    const names = ["Åsa", "Zara", "Ärla", "Anna", "Ödla", "Olga"];
    for (const title of names) await api.create("root", model, { title });
    const expected = ["Anna", "Olga", "Zara", "Åsa", "Ärla", "Ödla"];
    const firstColumn = async () =>
      (await page.locator("tbody tr td:nth-child(2)").allInnerTexts()).map((s) => s.trim());

    // Sorted by the server (the request carries the language).
    const sent = searches(page, model);
    const sort = JSON.stringify({ s: { f: "title", dir: "asc" } });
    await signIn("root", `/${model}/${model}?st=${encodeURIComponent(sort)}`);
    await expect.poll(firstColumn).toEqual(expected);
    expect(sent.some((b) => b.order?.[0]?.field === "title" && b.lang.startsWith("sv"))).toBe(true);

    // Sorted by the browser (a word in any field is not a server filter).
    const client = JSON.stringify({ s: { f: "title", dir: "asc" }, q: [{ op: "search", v: "a" }] });
    await page.goto(`/#/${model}/${model}?st=${encodeURIComponent(client)}`);
    await expect.poll(firstColumn).toEqual(expected);

    // Descending is the reverse.
    const desc = JSON.stringify({ s: { f: "title", dir: "desc" } });
    await page.goto(`/#/${model}/${model}?st=${encodeURIComponent(desc)}`);
    await expect.poll(firstColumn).toEqual([...expected].reverse());
  });
});

test.describe("views over many records do not load the table", () => {
  test("pivot, graph, kanban and calendar ask for aggregates and pages", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Many deals",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: STAGES },
        { name: "amount_minor", kind: "money" },
        { name: "due", kind: "date" },
      ],
    });
    await fill(api, model, 300, (i) => ({
      title: `Deal ${i}`,
      state: STAGES[i % 4],
      amount_minor: 100,
      due: `2026-10-${String(1 + (i % 28)).padStart(2, "0")}`,
    }));
    const sent = searches(page, model);
    await signIn("root", `/${model}/${model}?view=pivot`);

    // Pivot: 75 records and EUR 75.00 per stage, 300 in all.
    const pivot = page.locator("table.pivot-table");
    await expect(pivot.getByRole("row", { name: /^New/ })).toContainText("75");
    await expect(pivot.locator("tfoot")).toContainText("300");
    expect(sent.some((b) => b.aggregate)).toBe(true);

    // Graph as a table of data.
    await page.goto(`/#/${model}/${model}?view=graph`);
    await page.getByRole("button", { name: "Show data table" }).click();
    await expect(
      page.locator("table.pivot-table").getByRole("row", { name: /^Won/ }),
    ).toContainText("75");

    // Kanban: each column says 75 and shows its first cards; Load more raises it by a page.
    await page.goto(`/#/${model}/${model}?view=kanban`);
    const col = page.getByRole("region", { name: /^New, 75 cards$/ });
    await expect(col.locator(".count")).toHaveText("75");
    await expect(col.locator(".kcard")).toHaveCount(40);
    await col.getByRole("button", { name: "Load more (35 left)" }).click();
    await expect(col.locator(".kcard")).toHaveCount(75);
    await expect(col.getByRole("button", { name: /Load more/ })).toHaveCount(0);

    // Calendar: only the visible days are fetched (a date range in the filter).
    await page.goto(`/#/${model}/${model}?view=calendar`);
    await expect(page.locator(".calendar")).toBeVisible();
    await expect
      .poll(() =>
        sent.some(
          (b) =>
            b.filter?.some((c: any) => c.field === "due" && c.op === "gte") &&
            b.filter?.some((c: any) => c.field === "due" && c.op === "lte"),
        ),
      )
      .toBe(true);

    // None of it loaded the table: every request is an aggregate, a probe of one or a page of 40
    // or a month of the calendar, never the old 5,000-row load.
    for (const b of sent) expect(b.limit === undefined || b.limit <= 5000).toBe(true);
    expect(sent.filter((b) => b.limit === 5000 && !b.filter).length).toBe(0);
    expect(sent.filter((b) => !b.aggregate && b.limit === 40).length).toBeGreaterThan(0);
  });

  test("smart buttons count on the server", async ({ page, signIn, api, scratchModule }) => {
    const parent = await scratchModule({
      title: "Parents",
      fields: [{ name: "title", required: true }],
    });
    const child = await scratchModule({
      title: "Children",
      fields: [
        { name: "title", required: true },
        { name: "parent_id", kind: "ref", to: parent },
      ],
    });
    const p = await api.create("root", parent, { title: "Mum" });
    await fill(api, child, 45, (i) => ({ title: `Kid ${i}`, parent_id: p.id }));
    const sent = searches(page, child);
    await signIn("root", `/${parent}/${parent}?id=${p.id}`);
    await expect(page.getByRole("link", { name: /\(45\)/ })).toBeVisible();
    expect(sent.some((b) => b.aggregate)).toBe(true);
    expect(sent.some((b) => b.limit === 5000)).toBe(false);
  });
});

test.describe("changing the language keeps the screen", () => {
  test("an open record panel, its draft and the address survive", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
  }) => {
    const model = await scratchModule({
      title: "Keep things",
      fields: [
        { name: "title", required: true },
        { name: "note", kind: "longtext" },
      ],
    });
    const title = uid("Keep");
    const rec = await api.create("root", model, { title });
    await signIn("root", `/${model}/${model}?id=${rec.id}`);
    const panel = page.locator("aside.record");
    await expect(panel).toBeVisible();
    await panel.locator("textarea").fill("a draft not yet saved");
    // Mark the live DOM node: a remount would replace it.
    await panel.evaluate((el) => ((el as any).__mark = "same node"));
    const hash = await page.evaluate(() => location.hash);

    await page.getByRole("button", { name: "Account" }).click();
    await page.getByRole("menuitem", { name: "Deutsch" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "de");

    await expect(page.locator("aside.record")).toBeVisible();
    expect(await panel.evaluate((el) => (el as any).__mark)).toBe("same node");
    await expect(panel.locator("textarea")).toHaveValue("a draft not yet saved");
    expect(await page.evaluate(() => location.hash)).toBe(hash);
    // The text on screen is German now.
    await expect(panel.getByRole("button", { name: tr("de", "Close record") })).toBeVisible();
    await expect(page.getByRole("button", { name: "Close record" })).toHaveCount(0);

    // And back, still the same panel.
    await page.getByRole("button", { name: tr("de", "Account") }).click();
    await page.getByRole("menuitem", { name: "English" }).click();
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    expect(await panel.evaluate((el) => (el as any).__mark)).toBe("same node");
    await expect(panel.locator("textarea")).toHaveValue("a draft not yet saved");
  });
});

test.describe("failures are survivable", () => {
  test("a crash on one screen shows a recovery screen, the rest of the app keeps working", async ({
    page,
    signIn,
    api,
    scratchModule,
    allowConsoleErrors,
  }) => {
    allowConsoleErrors(/.*/);
    const model = await scratchModule({
      title: "Crashing",
      fields: [{ name: "title", required: true }],
    });
    await api.create("root", model, { title: "Fine" });
    let crash = true;
    await page.route(`**/api/${model}/search`, async (route) => {
      if (!crash) return route.continue();
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ value: { rows: [null], total: 1 }, denied: [] }),
      });
    });
    await signIn("root", `/${model}/${model}`);
    const recovery = page.getByTestId("recovery");
    await expect(recovery).toBeVisible();
    await expect(recovery).toContainText("Something went wrong on this screen");
    await expect(page.locator(".topbar")).toBeVisible();
    // Fixing the cause and pressing Try again brings the screen back.
    crash = false;
    await recovery.getByRole("button", { name: "Try again" }).click();
    await expect(page.getByRole("gridcell", { name: "Fine" })).toBeVisible();
  });

  test("a 500 shows a message and a way back, not a blank page", async ({
    page,
    signIn,
    api,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(500);
    const model = await scratchModule({
      title: "Broken",
      fields: [{ name: "title", required: true }],
    });
    await api.create("root", model, { title: "Alive" });
    let broken = true;
    await page.route(`**/api/${model}/search`, (route) =>
      broken
        ? route.fulfill({ status: 500, contentType: "text/html", body: "<h1>oops</h1>" })
        : route.continue(),
    );
    await signIn("root", `/${model}/${model}`);
    await expect(page.getByText("Could not load records")).toBeVisible();
    await expect(page.getByText("The server had a problem (status 500)")).toBeVisible();
    await expect(page.locator(".topbar")).toBeVisible();
    broken = false;
    await page.reload();
    await expect(page.getByRole("gridcell", { name: "Alive" })).toBeVisible();
  });

  test("a dropped connection shows the offline banner and recovers by itself", async ({
    page,
    signIn,
    api,
    scratchModule,
    allowConsoleErrors,
  }) => {
    allowConsoleErrors(/net::ERR|Failed to load resource/);
    const model = await scratchModule({
      title: "Flaky",
      fields: [{ name: "title", required: true }],
    });
    await api.create("root", model, { title: "Still here" });
    await signIn("root", `/${model}/${model}`);
    await expect(page.getByRole("gridcell", { name: "Still here" })).toBeVisible();

    let down = true;
    await page.route(`**/api/${model}/search`, (route) =>
      down ? route.abort("failed") : route.continue(),
    );
    await page.reload();
    await expect(page.getByText("Could not load records")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText("Could not reach the server")).toBeVisible();
    await expect(
      page.getByRole("alert").filter({ hasText: "The server cannot be reached" }),
    ).toBeVisible();

    down = false;
    await page.getByRole("alert").getByRole("button", { name: "Try again" }).click();
    await expect(
      page.getByRole("alert").filter({ hasText: "The server cannot be reached" }),
    ).toHaveCount(0);
  });

  test("a failing metadata request shows a recovery screen with a retry", async ({
    page,
    signIn,
    allowHttpErrors,
  }) => {
    allowHttpErrors(500);
    let broken = true;
    await page.route("**/api/meta", (route) =>
      broken ? route.fulfill({ status: 500, body: "boom" }) : route.continue(),
    );
    await signIn("root", "");
    await expect(page.getByText("Could not load your workspace")).toBeVisible();
    broken = false;
    await page.getByRole("button", { name: "Try again" }).click();
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
  });

  test("going offline in the browser shows the banner and going back online hides it", async ({
    page,
    signIn,
    context,
    allowConsoleErrors,
  }) => {
    allowConsoleErrors(/net::ERR/);
    await signIn("root", "");
    await context.setOffline(true);
    await expect(page.getByRole("alert").filter({ hasText: "You are offline" })).toBeVisible();
    await context.setOffline(false);
    await expect(page.getByRole("alert").filter({ hasText: "You are offline" })).toHaveCount(0);
  });
});

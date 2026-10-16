import type { Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";

// The bundled `dashboards` package stores dashboards and tiles as records; the app draws them.
// The package is installed for the test and removed afterwards, but the records it created stay in
// the database (uninstall keeps data), so every test starts by deleting them.
async function withDashboards(api: Api, run: () => Promise<void>) {
  const wipe = async () => {
    for (const w of await api.list("root", "dashboards_widget"))
      await api.run("root", "dashboards_widget.delete", { id: w.id });
    for (const d of await api.list("root", "dashboards_dashboard"))
      await api.run("root", "dashboards_dashboard.delete", { id: d.id });
  };
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "dashboards" }, root);
  try {
    await wipe();
    await run();
  } finally {
    await wipe().catch(() => {});
    await api.post("/api/module/uninstall", { id: "dashboards" }, root);
  }
}

type Tile = Record<string, unknown> & { title: string; kind: string; model_name: string };
async function board(api: Api, title: string, tiles: Tile[]) {
  const d = await api.create("root", "dashboards_dashboard", {
    title,
    owner: "root",
    shared: true,
    position: 10,
  });
  for (const [i, t] of tiles.entries())
    await api.create("root", "dashboards_widget", {
      aggregate: "count",
      ...t,
      dashboard_id: d.id,
      position: (i + 1) * 10,
    });
  return d;
}

const tile = (page: Page, title: string) => page.getByRole("article", { name: title, exact: true });
const value = (page: Page, title: string) => tile(page, title).locator(".stat-value");

async function source(
  api: Api,
  scratchModule: (s: any) => Promise<string>,
  uid: (p: string) => string,
) {
  const model = await scratchModule({
    title: "Deals",
    fields: [
      { name: "title", required: true },
      { name: "stage", kind: "select", options: ["open", "won", "lost"] },
      { name: "amount_minor", kind: "money" },
    ],
  });
  // Five records with known numbers: 2 open (10.00 + 25.00), 2 won (40.00 + 5.00), 1 lost (30.00).
  for (const [stage, amount] of [
    ["open", 1000],
    ["open", 2500],
    ["won", 4000],
    ["won", 500],
    ["lost", 3000],
  ] as const)
    await api.create("root", model, { title: uid("Deal"), stage, amount_minor: amount });
  return model;
}

test.describe("dashboards", () => {
  test("tiles render the numbers of the data behind them, and clicking one opens the filtered list", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await source(api, scratchModule, uid);
    await withDashboards(api, async () => {
      const base = { model_name: model };
      await board(api, "Known numbers", [
        { ...base, kind: "kpi", title: "All deals" },
        { ...base, kind: "kpi", title: "Won deals", filter: "stage = won" },
        {
          ...base,
          kind: "kpi",
          title: "Total amount",
          aggregate: "sum",
          measure_field: "amount_minor",
        },
        {
          ...base,
          kind: "kpi",
          title: "Big won deals",
          filter: "stage = won; amount_minor >= 1000",
        },
        { ...base, kind: "kpi", title: "Lost deals", filter: '{"stage":"lost"}' },
        { ...base, kind: "kpi", title: "Open or lost", filter: "stage in open|lost" },
        { ...base, kind: "bar", title: "Deals by stage", group_by: "stage" },
        {
          ...base,
          kind: "pie",
          title: "Amount by stage",
          group_by: "stage",
          aggregate: "sum",
          measure_field: "amount_minor",
        },
        { ...base, kind: "table", title: "Stage table", group_by: "stage" },
        { ...base, kind: "kpi", title: "Broken filter", filter: "stage is nonsense" },
      ]);
      await signIn("root", "/dashboards/board");

      await expect(value(page, "All deals")).toHaveText("5");
      await expect(value(page, "Won deals")).toHaveText("2");
      await expect(value(page, "Total amount")).toHaveText("€110.00");
      await expect(value(page, "Big won deals")).toHaveText("1");
      await expect(value(page, "Lost deals")).toHaveText("1");
      await expect(value(page, "Open or lost")).toHaveText("3");
      await expect(tile(page, "Broken filter")).toContainText("not set up correctly");

      // Charts carry the same numbers as marks with names, and as a table of their own.
      const bar = tile(page, "Deals by stage");
      await expect(bar.getByRole("button", { name: "Open: 2. Show records." })).toBeVisible();
      await expect(bar.getByRole("button", { name: "Won: 2. Show records." })).toBeVisible();
      await expect(bar.getByRole("button", { name: "Lost: 1. Show records." })).toBeVisible();
      const pie = tile(page, "Amount by stage");
      await expect(pie.getByRole("button", { name: /^Won: €45\.00, 41%/ })).toBeVisible();
      const table = tile(page, "Stage table");
      await expect(table.getByRole("row", { name: /Open\s*2/ })).toBeVisible();
      await expect(table.getByRole("row", { name: /Lost\s*1/ })).toBeVisible();

      // Drill-through: the tile opens the list with the same condition as a filter chip.
      await tile(page, "Won deals").getByRole("link").click();
      await expect(page.locator(".facet")).toHaveText(/Stage: Won/);
      await expect(page.locator(".controlpanel .count")).toHaveText("2 records");

      await page.goBack();
      await tile(page, "Deals by stage")
        .getByRole("button", { name: "Lost: 1. Show records." })
        .click();
      await expect(page.locator(".facet")).toHaveText(/Stage: Lost/);
      await expect(page.locator(".controlpanel .count")).toHaveText("1 record");
    });
  });

  test("a tile for data this person cannot see says No access and asks for nothing", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const hidden = await source(api, scratchModule, uid);
    await withDashboards(api, async () => {
      await board(api, "Mixed access", [
        { kind: "kpi", title: "Leads", model_name: "lead" },
        { kind: "kpi", title: "Hidden model", model_name: hidden },
        {
          kind: "kpi",
          title: "Hidden field",
          model_name: "product",
          aggregate: "sum",
          measure_field: "cost_minor",
        },
        { kind: "bar", title: "Hidden orders", model_name: "sale_order", group_by: "state" },
      ]);
      const asked: string[] = [];
      page.on("request", (r) => {
        if (r.url().includes("/api/") && r.method() === "POST")
          asked.push(r.url() + (r.postData() ?? ""));
      });
      await signIn("dana", "/dashboards/board");

      await expect(value(page, "Leads")).toBeVisible();
      for (const title of ["Hidden model", "Hidden field", "Hidden orders"]) {
        await expect(tile(page, title)).toContainText("No access");
        // Nothing of the tile leaks: no number, no chart, no name of the model or the field.
        await expect(
          tile(page, title).locator(".stat-value, .chartsvg, .piesvg, table"),
        ).toHaveCount(0);
        await expect(tile(page, title)).not.toContainText(/cost|sale_order|amount/i);
      }
      expect(
        asked.filter((u) => u.includes(`/api/${hidden}/`) || u.includes("/api/sale_order/")),
      ).toEqual([]);
      expect(asked.filter((u) => u.includes("/api/product/search"))).toEqual([]);

      // Dana can read dashboards but not change them.
      await expect(page.getByRole("button", { name: "Edit dashboard" })).toHaveCount(0);
    });
  });

  test("with no dashboard yet, a starter is built from the installed apps", async ({
    page,
    signIn,
    api,
  }) => {
    await withDashboards(api, async () => {
      await signIn("root", "/dashboards/board");
      await expect(page.getByRole("heading", { name: "No dashboards yet" })).toBeVisible();
      await page.getByRole("button", { name: "Create a starter dashboard" }).click();

      await expect(page.locator(".crumbs strong")).toHaveText("Overview");
      // Counts per app, money totals and charts: at least a handful of tiles with real numbers.
      await expect.poll(() => page.locator(".wcard").count()).toBeGreaterThanOrEqual(6);
      await expect(page.locator(".wcard .stat-value").first()).toHaveText(/^\d/);
      await expect(tile(page, "Lead").locator(".stat-value")).toHaveText(/^\d/);
      const tiles = await api.list("root", "dashboards_widget");
      expect(tiles.length).toBeGreaterThanOrEqual(6);
      expect(tiles.some((t) => t.aggregate === "sum")).toBe(true);
      expect(new Set(tiles.map((t) => t.dashboard_id)).size).toBe(1);
    });
  });

  test("edit mode adds, reorders and removes tiles, picking the model and field from what exists", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await source(api, scratchModule, uid);
    await withDashboards(api, async () => {
      await board(api, "Editable", [{ model_name: model, kind: "kpi", title: "First tile" }]);
      await signIn("root", "/dashboards/board");
      await expect(value(page, "First tile")).toHaveText("5");

      await page.getByRole("button", { name: "Edit dashboard" }).click();
      await expect(page).toHaveURL(/view=edit/);
      await page.getByRole("button", { name: "Add a tile" }).click();
      const form = page.getByRole("dialog", { name: "Add a tile" });
      // An empty form is refused with a message beside the field.
      await form.getByRole("button", { name: "Add tile" }).click();
      await expect(form.getByText("Give the tile a title")).toBeVisible();

      await form.getByLabel("Title").fill("Lost amount");
      await form.getByLabel("Records of").selectOption(model);
      await form.getByLabel("Combine by").selectOption("sum");
      // Only the number and money fields of the chosen model are offered to add up.
      await expect(form.getByLabel("Field to add up").locator("option")).toHaveText([
        "Choose a field",
        "Amount (money)",
      ]);
      await form.getByLabel("Field to add up").selectOption("amount_minor");
      await form.getByRole("button", { name: "Add a condition" }).click();
      await form.getByLabel("Field", { exact: true }).selectOption("stage");
      await form.getByLabel("Value").selectOption("lost");
      await expect(form.getByText("stage = lost").first()).toBeVisible();
      await form.getByRole("button", { name: "Add tile" }).click();

      await expect(page.locator(".toast").filter({ hasText: "Tile added" })).toBeVisible();
      await expect(value(page, "Lost amount")).toHaveText("€30.00");
      const stored = (await api.list("root", "dashboards_widget")).find(
        (w) => w.title === "Lost amount",
      );
      expect(stored).toMatchObject({
        kind: "kpi",
        model_name: model,
        aggregate: "sum",
        measure_field: "amount_minor",
        filter: "stage = lost",
      });

      // Reorder: the new tile moves before the first one, and the order is stored.
      const titles = () =>
        page.locator(".wcard").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
      expect(await titles()).toEqual(["First tile", "Lost amount"]);
      await page.getByRole("button", { name: "Move Lost amount earlier" }).click();
      await expect.poll(titles).toEqual(["Lost amount", "First tile"]);
      const positions = (await api.list("root", "dashboards_widget")).sort(
        (a, b) => a.position - b.position,
      );
      expect(positions.map((w) => w.title)).toEqual(["Lost amount", "First tile"]);

      // Remove asks first, in the app, and then the tile is gone.
      await page.getByRole("button", { name: "Remove First tile" }).click();
      const confirm = page.getByRole("alertdialog", { name: "Remove tile?" });
      await expect(confirm).toContainText("First tile");
      await confirm.getByRole("button", { name: "Remove tile" }).click();
      await expect(page.locator(".toast").filter({ hasText: "Removed" })).toBeVisible();
      await expect(tile(page, "First tile")).toHaveCount(0);
      expect(await api.list("root", "dashboards_widget")).toHaveLength(2 - 1);

      await page.getByRole("button", { name: "Done" }).click();
      await expect(page).not.toHaveURL(/view=edit/);
    });
  });
});

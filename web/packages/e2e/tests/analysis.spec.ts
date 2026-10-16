import { expect, test } from "../support/fixtures.ts";

const fields = [
  { name: "title", required: true },
  { name: "state", kind: "select" as const, options: ["draft", "confirmed", "cancelled"] },
  { name: "region", kind: "select" as const, options: ["north", "south"] },
  { name: "amount_minor", kind: "money" as const },
  { name: "due", kind: "date" as const },
];

// state, region, amount in cents. Draft 100.00 + 50.00, confirmed 200.00 + 25.50, cancelled 10.00.
const data = [
  ["draft", "north", 10000],
  ["draft", "south", 5000],
  ["confirmed", "north", 20000],
  ["confirmed", "north", 2550],
  ["cancelled", "south", 1000],
] as const;

test.describe("pivot and graph", () => {
  test("the pivot sums a measure by rows and columns with correct totals, and a cell drills into the list", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ title: "Pivot data", fields });
    for (const [state, region, amount] of data)
      await api.create("root", model, { title: uid("Row"), state, region, amount_minor: amount });
    await signIn("root", `/${model}/${model}?view=pivot`);

    const pivot = page.locator(".pivot");
    await pivot.getByLabel("Rows").selectOption("state");
    await pivot.getByLabel("Columns").selectOption("region");
    await pivot.getByLabel("Measure").selectOption("amount_minor");

    const table = page.getByRole("table");
    await expect(table.getByRole("columnheader", { name: "North" })).toBeVisible();
    await expect(table.getByRole("button", { name: "Draft, North: €100.00" })).toBeVisible();
    await expect(table.getByRole("button", { name: "Draft, South: €50.00" })).toBeVisible();
    await expect(table.getByRole("button", { name: "Confirmed, North: €225.50" })).toBeVisible();
    await expect(table.getByRole("button", { name: "Cancelled, South: €10.00" })).toBeVisible();
    // Row totals and the grand total.
    const totalRow = (name: string) => table.getByRole("row", { name: new RegExp(`^${name}`) });
    await expect(totalRow("Draft")).toContainText("€150.00");
    await expect(totalRow("Confirmed")).toContainText("€225.50");
    await expect(totalRow("Cancelled")).toContainText("€10.00");
    await expect(table.locator("tfoot")).toContainText("€385.50");

    // Count instead of a sum.
    await pivot.getByLabel("Measure").selectOption("count");
    await expect(table.locator("tfoot")).toContainText("5");
    await expect(table.getByRole("button", { name: "Confirmed, North: 2" })).toBeVisible();

    // A cell opens the records behind it.
    await table.getByRole("button", { name: "Confirmed, North: 2" }).click();
    await expect(page).toHaveURL(/view=list/);
    await expect(page.locator(".facet")).toHaveCount(2);
    await expect(page.locator(".controlpanel .count")).toHaveText("2 records");
  });

  test("the graph shows totals per group in its data table and as chart marks, and a mark drills into the list", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ title: "Graph data", fields });
    for (const [state, region, amount] of data)
      await api.create("root", model, { title: uid("Row"), state, region, amount_minor: amount });
    await signIn("root", `/${model}/${model}?view=graph`);

    const graph = page.locator(".graph");
    await graph.getByLabel("Dimension").selectOption("state");
    await graph.getByLabel("Measure").selectOption("amount_minor");
    await expect(graph.locator("figure.chart")).toBeVisible();
    await expect(
      graph
        .locator("g.mark")
        .filter({ has: page.locator("title, text") })
        .first(),
    ).toBeAttached();
    await expect(graph.locator('[aria-label*="Confirmed"]').first()).toHaveAttribute(
      "aria-label",
      /€225\.50/,
    );

    await graph.getByRole("button", { name: "Show data table" }).click();
    const table = graph.getByRole("table");
    await expect(table.getByRole("row", { name: /^Draft/ })).toContainText("€150.00");
    await expect(table.getByRole("row", { name: /^Confirmed/ })).toContainText("€225.50");
    await expect(table.getByRole("row", { name: /^Cancelled/ })).toContainText("€10.00");
    // Shares of 385.50: 150.00 is 39%.
    await expect(table.getByRole("row", { name: /^Draft/ })).toContainText("39%");

    await graph.getByRole("button", { name: "Show chart" }).click();
    await graph.getByRole("button", { name: "Pie" }).click();
    await expect(graph.locator("svg.piesvg")).toBeVisible();
    await graph.getByRole("button", { name: "Line" }).click();
    await expect(graph.locator("svg.chartsvg")).toBeVisible();
    await graph.getByRole("button", { name: "Bar" }).click();
    await graph.locator('g.mark[aria-label*="Cancelled"]').click();
    await expect(page).toHaveURL(/view=list/);
    await expect(page.locator(".controlpanel .count")).toHaveText("1 record");
  });
});

test.describe("calendar", () => {
  test("clicking a day creates a record on that date, which then shows on the day", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ title: "Calendar data", fields });
    // An empty model shows its empty state in every view, so start with one record in another year.
    await api.create("root", model, { title: uid("Old"), due: "2020-01-01" });
    const title = uid("Visit");
    await signIn("root", `/${model}/${model}?view=calendar`);

    const day = page.locator(".daycell:not(.out) .daynum", { hasText: /^15$/ });
    await day.click();
    const panel = page.getByRole("complementary", { name: /record$/ });
    await expect(panel.getByLabel("Due")).toHaveValue(/-15$/);
    await panel.getByLabel("Title").fill(title);
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();

    const cell = page
      .locator(".daycell", { has: page.locator(".daynum", { hasText: /^15$/ }) })
      .filter({ hasNot: page.locator(".out") });
    await expect(page.locator(".cal-ev", { hasText: title })).toHaveCount(1);
    await expect(cell.locator(".cal-ev", { hasText: title })).toHaveCount(1);

    // Clicking the event opens it; the week scale lists it too.
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Week" }).click();
    await page.getByRole("button", { name: "Month" }).click();
    await page.locator(".cal-ev", { hasText: title }).click();
    await expect(panel.getByRole("heading", { name: title })).toBeVisible();
  });
});

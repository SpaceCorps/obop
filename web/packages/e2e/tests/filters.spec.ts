import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

const fields = [
  { name: "title", required: true },
  { name: "stage", kind: "select" as const, options: ["new", "won", "lost"] },
  { name: "amount_minor", kind: "money" as const },
];
const rows = [
  ["Acme", "new", 1000],
  ["Acme Two", "new", 2000],
  ["Globex", "won", 3000],
  ["Initech", "won", 4000],
  ["Umbrella", "won", 5000],
  ["Hooli", "lost", 6000],
] as const;

async function seeded(page: Page, ctx: { signIn: any; api: any; scratchModule: any }) {
  const model: string = await ctx.scratchModule({ title: "Filter board", fields });
  for (const [title, stage, amount] of rows)
    await ctx.api.create("root", model, { title, stage, amount_minor: amount });
  await ctx.signIn("root", `/${model}/${model}?view=list`);
  await expect(count(page)).toHaveText("6 records");
  return model;
}
const count = (page: Page) => page.locator(".controlpanel .count");
const panelButton = (page: Page) =>
  page.getByRole("button", { name: "Filters, group by and favorites" });

test.describe("search, filters, group by and favorites", () => {
  test("a search facet narrows the list, lands in the URL and is removed with its chip", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    await seeded(page, { signIn, api, scratchModule });
    const search = page.getByRole("combobox", { name: /^Search/ });
    await search.fill("acme");
    await search.press("Enter");
    await expect(count(page)).toHaveText("2 records");
    await expect(page.locator(".facet")).toHaveText(/Search: acme/);
    await expect(page).toHaveURL(/st=/);

    await page.getByRole("button", { name: /^Remove filter/ }).click();
    await expect(count(page)).toHaveText("6 records");
    await expect(page.locator(".facet")).toHaveCount(0);
  });

  test("quick filters OR within a field, a custom range ANDs on top, and the empty state offers Clear filters", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    await seeded(page, { signIn, api, scratchModule });
    await panelButton(page).click();
    await page.getByRole("checkbox", { name: "Stage: Won" }).check();
    await expect(count(page)).toHaveText("3 records");
    await page.getByRole("checkbox", { name: "Stage: Lost" }).check();
    await expect(count(page)).toHaveText("4 records");

    await page.getByRole("button", { name: "Add custom filter" }).click();
    await page.getByLabel("Filter field").selectOption("amount_minor");
    await page.getByLabel("From", { exact: true }).fill("35");
    await page.getByLabel("To", { exact: true }).fill("55");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(count(page)).toHaveText("2 records");

    await page.getByRole("checkbox", { name: "Stage: Lost" }).uncheck();
    await page.getByRole("checkbox", { name: "Stage: Won" }).uncheck();
    await page.getByRole("checkbox", { name: "Stage: New" }).check();
    await expect(page.getByText("No matches")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Clear filters" }).click();
    await expect(count(page)).toHaveText("6 records");
  });

  test("group by shows a header per stage with its sum, and groups collapse", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    await seeded(page, { signIn, api, scratchModule });
    await panelButton(page).click();
    await page.getByRole("checkbox", { name: "Stage", exact: true }).check();
    await page.keyboard.press("Escape");

    const heads = page.locator("tr.ghead");
    await expect(heads).toHaveCount(3);
    await expect(heads.filter({ hasText: "Won" })).toContainText("€120.00");
    await expect(heads.filter({ hasText: "New" })).toContainText("€30.00");
    await expect(heads.filter({ hasText: "Lost" })).toContainText("€60.00");

    await heads.filter({ hasText: "Won" }).locator(".gbtn").click();
    await expect(heads.filter({ hasText: "Won" }).locator(".gbtn")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    await expect(page.getByRole("row", { name: /Globex/ })).toHaveCount(0);
    await expect(page.getByRole("row", { name: /Hooli/ })).toBeVisible();
  });

  test("a saved favorite restores filters and grouping, a default favorite applies on open, and the URL alone restores a view", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await seeded(page, { signIn, api, scratchModule });
    await panelButton(page).click();
    await page.getByRole("checkbox", { name: "Stage: Won" }).check();
    await page.getByRole("checkbox", { name: "Stage", exact: true }).check();
    await expect(count(page)).toHaveText("3 records");
    await page.getByLabel("Name for this search").fill("Won deals");
    await page.getByLabel("Use by default").check();
    await page.getByRole("button", { name: "Save current search" }).click();
    await expect(page.getByRole("button", { name: "Won deals", exact: true })).toBeVisible();
    const withState = page.url();
    expect(withState).toContain("st=");
    await page.keyboard.press("Escape");

    // The address alone brings the same view back from a cold load (favorites aside).
    await page.goto("/#/");
    await page.goto(withState);
    await page.reload();
    await expect(count(page)).toHaveText("3 records");
    await expect(page.locator(".facet").first()).toContainText("Stage");

    // Opening the model without any state applies the default favorite.
    await page.goto(`/#/${model}/${model}?view=list`);
    await page.reload();
    await expect(count(page)).toHaveText("3 records");

    // Clear everything, then restore with the favorite.
    await panelButton(page).click();
    await page.getByRole("button", { name: "Clear all" }).click();
    await expect(count(page)).toHaveText("6 records");
    await page.getByRole("button", { name: "Won deals", exact: true }).click();
    await expect(count(page)).toHaveText("3 records");
    await expect(page.locator("tr.ghead")).toHaveCount(1);

    // Delete it again.
    await panelButton(page).click();
    await page.getByRole("button", { name: "Delete Won deals" }).click();
    await expect(page.getByRole("button", { name: "Won deals", exact: true })).toHaveCount(0);
  });

  test("sorting by a column header orders the rows and toggles direction", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    await seeded(page, { signIn, api, scratchModule });
    const first = () => page.locator('tbody tr[role="row"] td[data-c="1"]').first();
    await page.getByRole("button", { name: /^Amount/ }).click();
    await expect(first()).toHaveText("Acme");
    await page.getByRole("button", { name: /^Amount/ }).click();
    await expect(first()).toHaveText("Hooli");
  });
});

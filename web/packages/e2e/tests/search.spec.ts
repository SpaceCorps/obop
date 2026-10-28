import type { Page, Request } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";
import { isCommand } from "../support/urls.ts";

// The list pages, filters and sorts on the server (D49). One throwaway module with 120 records, so
// every number below is the only data there is. Row 001 is stage a, Row 002 is b, Row 003 is c, and
// so on; the amount is 1.00 EUR for each step of the row number.
const STAGES = ["a", "b", "c"];
const COUNT = 120;
const pad = (n: number) => String(n).padStart(3, "0");

async function seed(api: Api, scratchModule: (s: any) => Promise<string>) {
  const model = await scratchModule({
    title: "Paging",
    fields: [
      { name: "title", required: true },
      { name: "stage", kind: "select", options: STAGES },
      { name: "amount_minor", kind: "money" },
    ],
  });
  // One after the other: the default order of a list is insertion order.
  for (let n = 1; n <= COUNT; n++)
    await api.create("root", model, {
      title: `Row ${pad(n)}`,
      stage: STAGES[(n - 1) % 3],
      amount_minor: n * 100,
    });
  return model;
}

const searches = (page: Page, model: string) => {
  const seen: Record<string, any>[] = [];
  page.on("request", (r: Request) => {
    if (isCommand(r.url(), model, "search") && r.postData()) seen.push(JSON.parse(r.postData()!));
  });
  return seen;
};
const range = (page: Page) => page.locator(".pager-range");
const rowTitles = async (page: Page) =>
  (await page.locator("table.list tbody tr:not(.ghead) td[data-c='1']").allInnerTexts()).map((t) =>
    t.trim(),
  );

test.describe("server-side list: paging, filter, sort", () => {
  test("pages through 120 records on the server, with a page size choice and the page in the URL", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await seed(api, scratchModule);
    const seen = searches(page, model);
    await signIn("root", `/${model}/${model}?view=list`);

    // The default page holds 100 and the control panel counts every record.
    await expect(page.locator(".controlpanel .count")).toHaveText("120 records");
    await expect(range(page)).toHaveText("1–100 of 120");
    expect(await page.locator("table.list tbody tr").count()).toBe(100);

    await page.getByLabel("Rows per page").selectOption("25");
    await expect(range(page)).toHaveText("1–25 of 120");
    expect(await rowTitles(page)).toHaveLength(25);
    await expect(page.getByRole("button", { name: "Previous page" })).toBeDisabled();

    const next = page.getByRole("button", { name: "Next page" });
    await next.click();
    await expect(range(page)).toHaveText("26–50 of 120");
    await expect(page).toHaveURL(/page=2/);
    await expect.poll(async () => (await rowTitles(page))[0]).toBe("Row 026");
    // The server did the paging: the last request asks for 25 records after the first 25.
    expect(seen.at(-1)).toMatchObject({ limit: 25, offset: 25 });

    for (let i = 0; i < 3; i++) await next.click();
    await expect(range(page)).toHaveText("101–120 of 120");
    await expect.poll(async () => (await rowTitles(page)).length).toBe(20);
    await expect(next).toBeDisabled();

    // The address alone restores the page, and a new search starts again on page one.
    await page.goto(`/#/${model}/${model}?view=list&page=3`);
    await expect(range(page)).toHaveText("51–75 of 120");
    await expect.poll(async () => (await rowTitles(page))[0]).toBe("Row 051");
    const box = page.getByRole("combobox", { name: /^Search/ });
    await box.fill("Row 11");
    await box.press("Enter");
    await expect(page).not.toHaveURL(/page=/);
  });

  test("a filter and a sort are sent to the server and the pager follows the matches", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await seed(api, scratchModule);
    const seen = searches(page, model);
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(range(page)).toHaveText("1–100 of 120");

    await page.getByRole("button", { name: "Filters, group by and favorites" }).click();
    await page.getByRole("checkbox", { name: "Stage: B" }).check();
    await expect(page.locator(".controlpanel .count")).toHaveText("40 records");
    await expect(range(page)).toHaveText("1–40 of 40");
    expect(seen.at(-1)?.filter).toEqual([{ field: "stage", op: "eq", value: "b" }]);
    await page.keyboard.press("Escape");

    // Sorting by Amount, twice for descending, orders all 40 on the server, not only this page.
    const head = page.getByRole("columnheader", { name: /^Amount/ });
    await head.getByRole("button").click();
    await head.getByRole("button").click();
    await expect.poll(async () => (await rowTitles(page))[0]).toBe("Row 119");
    expect(seen.at(-1)?.order).toEqual([{ field: "amount_minor", dir: "desc" }]);
    await expect(head).toHaveAttribute("aria-sort", "descending");

    // Page size 25 splits the 40 matches in two.
    await page.getByLabel("Rows per page").selectOption("25");
    await expect(range(page)).toHaveText("1–25 of 40");
    await page.getByRole("button", { name: "Next page" }).click();
    await expect(range(page)).toHaveText("26–40 of 40");
    await expect.poll(async () => (await rowTitles(page))[0]).toBe("Row 044");
  });

  test("what the server cannot express runs in the browser: search everywhere and grouping", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await seed(api, scratchModule);
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(range(page)).toHaveText("1–100 of 120");

    // "Search everywhere" is one word across all fields: the server has no such condition.
    const box = page.getByRole("combobox", { name: /^Search/ });
    await box.fill("Row 07");
    await box.press("Enter");
    await expect(page.locator(".controlpanel .count")).toHaveText("10 records");
    expect((await rowTitles(page))[0]).toBe("Row 070");
    await page.getByRole("button", { name: /^Remove filter/ }).click();
    await expect(page.locator(".controlpanel .count")).toHaveText("120 records");

    // Grouping needs every record: three groups, no pager.
    await page.getByRole("button", { name: "Filters, group by and favorites" }).click();
    await page.getByRole("checkbox", { name: "Stage", exact: true }).check();
    await page.keyboard.press("Escape");
    await expect(page.locator("tr.ghead")).toHaveCount(3);
    await expect(page.locator("tr.ghead").first()).toContainText("40");
    await expect(page.locator(".pager")).toHaveCount(0);
  });

  test("a search on a field the caller may not read is explained, not a crash", async ({
    page,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: ["base.user"], label: "search agent" },
      await api.token("sam"),
    );
    await page.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [issued.json.token, "agent"],
    );
    // Names are personal data: an agent sees them as tokens, so the server refuses to filter on them.
    const st = encodeURIComponent(JSON.stringify({ q: [{ op: "contains", f: "name", v: "x" }] }));
    await page.goto(`/#/base/party?view=list&st=${st}`);
    await expect(
      page.getByRole("heading", { name: "This search uses a field you cannot read" }),
    ).toBeVisible();
    await expect(page.getByText(/unknown field party\.name/)).toBeVisible();

    await page.getByRole("button", { name: "Clear filters and sorting" }).click();
    await expect(page.getByRole("grid", { name: "Party list" })).toBeVisible();
    await expect(page.locator(".facet")).toHaveCount(0);
  });
});

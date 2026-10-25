import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// D72: exact decimals and instants in the backoffice. A throwaway module with a decimal (two
// digits after the point), a decimal that rounds half up when imported, and a datetime.
const fields = [
  { name: "title", required: true },
  { name: "amount", kind: "decimal" as const, precision: 12, scale: 2 },
  { name: "ratio", kind: "decimal" as const, precision: 10, scale: 4, round: "half_up" as const },
  { name: "booked_at", kind: "datetime" as const },
];
const count = (page: Page) => page.locator(".controlpanel .count");
const row = (page: Page, text: string) =>
  page
    .getByRole("grid")
    .first()
    .getByRole("row", { name: new RegExp(text) });

test.describe("decimal and datetime fields", () => {
  test("a decimal is shown exactly, ordered as a number and filtered as one", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model: string = await scratchModule({ title: "Ledger", fields });
    for (const [title, amount] of [
      ["Small", "9.50"],
      ["Big", "10.25"],
      ["Huge", "1234567.05"],
      ["Debt", "-3.00"],
    ] as const)
      await api.create("root", model, { title, amount });
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(count(page)).toHaveText("4 records");
    // Thousands separators of the locale, always two digits after the point.
    await expect(row(page, "Huge")).toContainText("1,234,567.05");
    await expect(row(page, "Small")).toContainText("9.50");

    // Sorting by the amount is numeric: 9.50 before 10.25 (as text it would be the other way).
    await page
      .getByRole("columnheader", { name: /Amount/ })
      .getByRole("button")
      .first()
      .click();
    const titles = async () =>
      (await page.getByRole("grid").first().getByRole("row").allInnerTexts())
        .map((t) => /Small|Big|Huge|Debt/.exec(t)?.[0])
        .filter(Boolean);
    expect(await titles()).toEqual(["Debt", "Small", "Big", "Huge"]);

    // A range filter compares exactly, on the server.
    await page.getByRole("button", { name: "Filters, group by and favorites" }).click();
    await page.getByRole("button", { name: "Add custom filter" }).click();
    await page.getByLabel("Filter field").selectOption("amount");
    await page.getByLabel("From", { exact: true }).fill("9.7");
    await page.getByLabel("To", { exact: true }).fill("20");
    await page.getByRole("button", { name: "Add", exact: true }).click();
    await expect(count(page)).toHaveText("1 record");
    await expect(row(page, "Big")).toBeVisible();
  });

  test("the form takes a decimal as typed, refuses extra digits and takes an instant with an offset", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model: string = await scratchModule({ title: "Ledger form", fields });
    const title = uid("Booked");
    await signIn("root", `/${model}/${model}?view=list&id=new`);
    const panel = page.getByRole("complementary").first();
    await panel.getByLabel("Title").fill(title);
    await panel.getByLabel("Amount").fill("12.345");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(
      panel.getByRole("alert").filter({ hasText: "at most 2 digits after the point" }),
    ).toBeVisible();
    await panel.getByLabel("Amount").fill("12.5");
    await panel.getByLabel("Booked at").fill("29 March");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(
      panel.getByRole("alert").filter({ hasText: "must be a date and time like" }),
    ).toBeVisible();
    // The instance runs in UTC in the test suite; an offset says another zone outright.
    await panel.getByLabel("Booked at").fill("2026-03-29 03:30+02:00");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast")).toContainText("created");
    const saved = (await api.list("root", model)).find((r) => r.title === title);
    expect(saved.amount).toBe("12.50");
    expect(saved.booked_at).toBe("2026-03-29T01:30:00.000000Z");
  });

  test("import rounds where the field says so and refuses extra digits elsewhere", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model: string = await scratchModule({ title: "Ledger import", fields });
    const good = uid("Rounded");
    const bad = uid("TooPrecise");
    await signIn("root", `/${model}/${model}?view=list`);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: /^Import .* from a CSV file$/ });
    await expect(dialog.getByText(/Decimal numbers are written with a point/)).toBeVisible();
    const file = (body: string) => ({
      name: "ledger.csv",
      mimeType: "text/csv",
      buffer: Buffer.from(body),
    });
    await dialog
      .getByLabel("Choose a CSV file")
      .setInputFiles(
        file(
          `title,amount,ratio,booked_at\n${good},1.00,0.12345,2026-03-29T03:30:00+02:00\n${bad},1.005,0.1,2026-03-29T01:30:00Z\n`,
        ),
      );
    const problems = dialog.getByRole("table", { name: "Problems found" });
    await expect(problems).toContainText("3 fractional digits");
    await dialog.getByLabel("If a row is wrong").selectOption("partial");
    await dialog.getByRole("button", { name: "Import 1 row" }).click();
    await expect(dialog.getByText(/Created: 1\. Skipped: 1\./)).toBeVisible();
    const rows = await api.list("root", model);
    const made = rows.find((r) => r.title === good);
    expect(made.ratio).toBe("0.1235");
    expect(made.booked_at).toBe("2026-03-29T01:30:00.000000Z");
    expect(rows.some((r) => r.title === bad)).toBe(false);
  });
});

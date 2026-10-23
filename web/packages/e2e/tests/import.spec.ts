import { expect, open, test } from "../support/fixtures.ts";

// D70: import records from a CSV file in the list toolbar. The dialog checks the file first (a
// dry run), says which rows are wrong, and writes nothing until the person confirms.
const leadRow = (page: import("@playwright/test").Page, title: string) =>
  page.getByRole("grid", { name: "Lead list" }).getByRole("row", { name: new RegExp(title) });

test.describe("CSV import", () => {
  test("check a file, see what is wrong, import the good rows, and a double click imports once", async ({
    page,
    api,
    signIn,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const a = uid("Import A");
    const b = uid("Import B");
    const c = uid("Import C");
    await signIn("root");
    await open(page, "/crm/lead?view=list");
    await page.getByRole("button", { name: "Import", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: /^Import .* from a CSV file$/ });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/Fields you can fill:.*title/)).toBeVisible();
    // The import button is not offered before a file has been checked.
    await expect(dialog.getByRole("button", { name: "Import", exact: true })).toBeDisabled();

    // A file with one bad row (no title, a decimal amount): the dialog names the rows.
    const file = (body: string) => ({
      name: "leads.csv",
      mimeType: "text/csv",
      buffer: Buffer.from(body),
    });
    const bad = `title;contact_name;expected_revenue_minor\r\n${a};Ann Example;125000\r\n;No Title;100\r\n${b};Ben Example;12.50\r\n${c};Cy Example;\r\n`;
    await dialog.getByLabel("Choose a CSV file").setInputFiles(file(bad));
    await expect(dialog.getByText(/Rows ready: 2\. Rows with problems: 2\./)).toBeVisible();
    const problems = dialog.getByRole("table", { name: "Problems found" });
    await expect(problems).toContainText("lead.title is required");
    await expect(problems).toContainText("without a decimal point");
    await expect(dialog.getByRole("button", { name: "Import", exact: true })).toBeDisabled();
    expect((await api.list("root", "lead")).some((l) => l.title === a)).toBe(false);

    // Skip the bad rows: the good ones import, once, whatever the number of clicks.
    await dialog.getByLabel("If a row is wrong").selectOption("partial");
    const go = dialog.getByRole("button", { name: "Import 2 rows" });
    await expect(go).toBeEnabled();
    await go.dblclick();
    await expect(dialog.getByText(/Created: 2\. Skipped: 2\./)).toBeVisible();
    await dialog.getByRole("button", { name: "Done" }).click();
    await expect(leadRow(page, a)).toContainText("Ann Example");
    await expect(leadRow(page, a)).toContainText("€1,250.00");
    await expect(leadRow(page, c)).toBeVisible();
    await expect(leadRow(page, c)).toContainText("New");
    expect((await api.list("root", "lead")).filter((l) => l.title === a)).toHaveLength(1);

    // The stored rows are the ones the file named; the bad ones are not there.
    expect((await api.list("root", "lead")).some((l) => l.title === b)).toBe(false);
  });

  test("a person who may read but not create sees no Import button", async ({
    page,
    api,
    signIn,
  }) => {
    // sam holds product.user, which reads products; creating them needs product.manager.
    await signIn("sam", "/product/product?view=list");
    await expect(page.getByRole("grid").first()).toBeVisible();
    await expect(page.getByRole("button", { name: "New", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Import", exact: true })).toHaveCount(0);
    const refused = await api.post(
      "/api/product/bulk_create",
      { rows: [{ name: "x" }] },
      await api.token("sam"),
    );
    expect(refused.status).toBe(404);
    // Where he can create, the button is there.
    await open(page, "/crm/lead?view=list");
    await expect(page.getByRole("button", { name: "Import", exact: true })).toBeVisible();
  });
});

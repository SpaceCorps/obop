import { expect, open, test } from "../support/fixtures.ts";

const leadRow = (page: import("@playwright/test").Page, title: string) =>
  page.getByRole("grid", { name: "Lead list" }).getByRole("row", { name: new RegExp(title) });

test.describe("records in a built-in module (CRM leads)", () => {
  test("create a lead: required field is validated, then the record appears in the list", async ({ page, signIn, uid }) => {
    const title = uid("Create lead");
    await signIn("root");
    await open(page, "/crm/lead?view=list");
    await page.getByRole("button", { name: "New", exact: true }).click();
    const panel = page.getByRole("complementary", { name: "Lead record" });
    await expect(panel.getByRole("heading", { name: "New lead" })).toBeVisible();

    // Title is required: submitting empty shows the message beside the field and creates nothing.
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(panel.getByRole("alert").filter({ hasText: "Title is required" })).toBeVisible();
    await expect(page.locator(".toast")).toContainText("Fix the highlighted fields first");

    await panel.getByLabel("Title").fill(title);
    await panel.getByLabel("Contact name").fill("Dana Example");
    await panel.getByLabel("Expected revenue").fill("1234.50");
    await panel.getByRole("button", { name: "Qualified" }).click();
    await panel.getByRole("button", { name: "Create" }).click();

    await expect(page.locator(".toast").filter({ hasText: "Lead created" })).toBeVisible();
    const row = leadRow(page, title);
    await expect(row).toBeVisible();
    await expect(row).toContainText("Dana Example");
    await expect(row).toContainText("€1,234.50");
    await expect(row).toContainText("Qualified");
    // The panel stays open on the saved record, now in edit mode.
    await expect(panel.getByRole("heading", { name: title })).toBeVisible();
    await expect(panel.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  test("a bad amount is caught in the form; a number the server refuses is shown inline from its 400", async ({
    page,
    signIn,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const title = uid("Invalid lead");
    await signIn("root", "/crm/lead?view=list&id=new");
    const panel = page.getByRole("complementary", { name: "Lead record" });
    await panel.getByLabel("Title").fill(title);

    await panel.getByLabel("Expected revenue").fill("abc");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(panel.getByRole("alert").filter({ hasText: "must be an amount like 12.50" })).toBeVisible();

    // The form accepts this amount, but it does not fit the server's integer: its message lands on the field.
    await panel.getByLabel("Expected revenue").fill("99999999999999999999.99");
    await panel.getByRole("button", { name: "Create" }).click();
    const inline = panel.getByRole("alert").filter({ hasText: "lead.expected_revenue_minor" });
    await expect(inline).toBeVisible();
    await expect(inline).toContainText("integer");
    await expect(panel.getByLabel("Expected revenue")).toHaveAttribute("aria-invalid", "true");
    await expect(leadRow(page, title)).toHaveCount(0);

    // Fixing the value clears the message and saves.
    await panel.getByLabel("Expected revenue").fill("10");
    await expect(inline).toHaveCount(0);
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(leadRow(page, title)).toContainText("€10.00");
  });

  test("edit a lead: Save persists it and Discard throws the draft away", async ({ page, signIn, api, uid }) => {
    const title = uid("Edit lead");
    await api.create("root", "lead", { title, stage: "new", expected_revenue_minor: 5000 });
    await signIn("root", "/crm/lead?view=list");
    await leadRow(page, title).getByRole("gridcell").nth(1).click();
    const panel = page.getByRole("complementary", { name: "Lead record" });
    await expect(panel.getByRole("heading", { name: title })).toBeVisible();
    await expect(panel.getByRole("button", { name: "Save" })).toBeDisabled();

    await panel.getByLabel("Title").fill(`${title} renamed`);
    await expect(panel.getByRole("button", { name: "Save" })).toBeEnabled();
    await panel.getByRole("button", { name: "Discard" }).click();
    await expect(panel.getByLabel("Title")).toHaveValue(title);

    await panel.getByLabel("Title").fill(`${title} renamed`);
    await panel.getByLabel("Expected revenue").fill("75");
    await page.keyboard.press("ControlOrMeta+s");
    await expect(page.locator(".toast").filter({ hasText: "Saved" })).toBeVisible();
    const row = leadRow(page, `${title} renamed`);
    await expect(row).toContainText("€75.00");

    const stored = (await api.list("root", "lead")).find((l) => l.title === `${title} renamed`);
    expect(stored?.expected_revenue_minor).toBe(7500);
  });

  test("leaving a form with unsaved edits asks first, and Keep editing keeps the draft", async ({ page, signIn, api, uid }) => {
    const title = uid("Guarded lead");
    await api.create("root", "lead", { title });
    await signIn("root", "/crm/lead?view=list");
    await leadRow(page, title).getByRole("gridcell").nth(1).click();
    const panel = page.getByRole("complementary", { name: "Lead record" });
    await panel.getByLabel("Title").fill("Unsaved change");

    await page.keyboard.press("Escape");
    const confirm = page.getByRole("alertdialog").or(page.locator(".confirm"));
    await expect(confirm).toBeVisible();
    await page.getByRole("button", { name: "Keep editing" }).click();
    await expect(panel.getByLabel("Title")).toHaveValue("Unsaved change");

    await page.getByRole("button", { name: "Discard" }).click();
    await page.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
  });

  test("an action button runs a command on the record", async ({ page, signIn, api, uid }) => {
    const title = uid("Won lead");
    const lead = await api.create("root", "lead", { title, stage: "new" });
    await signIn("root", `/crm/lead?view=list&id=${lead.id}`);
    const panel = page.getByRole("complementary", { name: "Lead record" });
    await panel.getByRole("button", { name: "Mark won" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Mark won done" })).toBeVisible();
    await expect(leadRow(page, title)).toContainText("Won");
  });
});

test.describe("inline edit in the list", () => {
  test("click a cell, type, Enter saves; Escape cancels; a bad value shows an inline error", async ({ page, signIn, api, uid }) => {
    const title = uid("Inline lead");
    await api.create("root", "lead", { title, contact_name: "Original Name", expected_revenue_minor: 1000 });
    await signIn("root", "/crm/lead?view=list");
    const row = leadRow(page, title);
    const cell = (n: number) => row.locator(`td[data-c="${n}"]`);
    await expect(cell(2)).toHaveText("Original Name");

    await cell(2).click();
    const input = page.locator("td.editing input");
    await expect(input).toBeFocused();
    await input.fill("Edited Name");
    await page.keyboard.press("Enter");
    await expect(cell(2)).toHaveText("Edited Name");

    await cell(2).click();
    await page.locator("td.editing input").fill("Never saved");
    await page.keyboard.press("Escape");
    await expect(cell(2)).toHaveText("Edited Name");

    await cell(4).click();
    await page.locator("td.editing input").fill("not money");
    await page.keyboard.press("Enter");
    await expect(page.locator(".ie-error")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".ie-error")).toHaveCount(0);

    // The stored value is what the list shows.
    const stored = (await api.list("root", "lead")).find((l) => l.title === title);
    expect(stored?.contact_name).toBe("Edited Name");
    expect(stored?.expected_revenue_minor).toBe(1000);
  });
});

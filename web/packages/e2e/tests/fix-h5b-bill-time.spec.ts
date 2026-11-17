import { expect, test } from "../support/fixtures.ts";

// "Bill time" on a project (docs/ISSUES.md K62): the project names its customer and hourly rate
// (projects 0.6.0), and the button turns its approved, billable time of a period into a draft
// invoice through `time_billing.create_invoice`, first as a preview (the dry run).

test("a project's approved time becomes a draft invoice from the project (K62)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  for (const id of ["projects", "timesheets"])
    if (!had.has(id)) await api.run("root", "marketplace_listing.install", { id });
  try {
    const customer = await api.create("root", "party", { name: uid("Werft") });
    const project = await api.create("root", "projects_project", {
      name: uid("Training"),
      customer_id: customer.id,
      hourly_rate: 9500,
    });
    const task = await api.create("root", "projects_task", {
      title: uid("Session"),
      project_id: project.id,
    });
    // An approved, billable entry in the month before today (the dialog's period).
    const now = new Date();
    const day = new Date(Date.UTC(now.getFullYear(), now.getMonth() - 1, 10))
      .toISOString()
      .slice(0, 10);
    const entry = await api.create("root", "timesheets_entry", {
      task_id: task.id,
      worker: "ann",
      worked_on: day,
      minutes: 90,
      billable: true,
    });
    await api.run("root", "timesheets_entry.submit", { id: entry.id });
    await api.run("root", "timesheets_entry.approve", { id: entry.id });

    await signIn("root", `/projects/projects_project?id=${project.id}`);
    await page.getByRole("button", { name: "Bill time" }).click();
    const dialog = page.getByRole("dialog", { name: "Bill time" });
    await expect(dialog.getByRole("combobox", { name: "Customer" })).toHaveValue(customer.name);
    await expect(dialog.getByLabel(/Hourly rate/)).toHaveValue("95.00");
    // The period is typed as every other date of the back office (the region's order, with its
    // calendar), not in the browser's own date box.
    await expect(dialog.locator('input[type="date"]')).toHaveCount(0);
    const from = dialog.getByRole("textbox", { name: "From" });
    await expect(from).not.toHaveValue(/^\d{4}-/);
    await expect(from).toHaveValue(/\d{2}.\d{2}.\d{4}/);
    await dialog.getByRole("button", { name: "Preview" }).click();
    await expect(dialog.locator(".prj-bill-preview")).toContainText("1.50 h (1 entry)");
    await expect(dialog.locator(".prj-bill-total")).toContainText("142.50");
    await dialog.getByRole("button", { name: "Create draft invoice" }).click();
    await expect(dialog).toBeHidden();
    const billed = (await api.run("root", "time_billing.get", { entry: entry.id })) as any;
    expect(billed.billed).toBe(true);
    const invoice = (await api.run("root", "invoice.get", { id: billed.invoice_id })) as any;
    expect(invoice.state).toBe("draft");
    expect(invoice.party_id).toBe(customer.id);
  } finally {
    for (const id of ["timesheets", "projects"])
      if (!had.has(id)) await api.post("/api/module/uninstall", { id }, root);
  }
});

test("a project without a customer or a rate has them asked in words, not by field id", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  for (const id of ["projects", "timesheets"])
    if (!had.has(id)) await api.run("root", "marketplace_listing.install", { id });
  try {
    const project = await api.create("root", "projects_project", { name: uid("Bare") });
    await signIn("root", `/projects/projects_project?id=${project.id}`);
    await page.getByRole("button", { name: "Bill time" }).click();
    const dialog = page.getByRole("dialog", { name: "Bill time" });
    await dialog.getByRole("button", { name: "Preview" }).click();
    await expect(dialog).toContainText("Choose the customer to bill");
    const customer = await api.create("root", "party", { name: uid("Werft") });
    await dialog.getByRole("combobox", { name: "Customer" }).fill(customer.name);
    await page.getByRole("option", { name: customer.name }).first().click();
    await dialog.getByRole("button", { name: "Preview" }).click();
    await expect(dialog).toContainText("Enter the hourly rate as an amount, such as 95.00");
    await expect(dialog).not.toContainText("hourly_rate_minor");
  } finally {
    for (const id of ["timesheets", "projects"])
      if (!had.has(id)) await api.post("/api/module/uninstall", { id }, root);
  }
});

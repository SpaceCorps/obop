import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// D56 to D58 through the real app: effects, computed totals, gapless numbers, unique values and
// row-level access.

const panelOf = (page: Page, name: string) =>
  page.getByRole("complementary", { name: `${name} record` });

test.describe("the data model", () => {
  test("confirming an order creates an invoice with its lines and total, and posting numbers it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const party = await api.create("root", "party", { name: uid("Model customer") });
    const order = await api.create("root", "sale_order", { party_id: party.id, currency: "EUR" });
    const line = await api.create("root", "sale_order_line", {
      order_id: order.id,
      description: "Workshop",
      quantity: 2,
      unit_price_minor: 50000,
    });
    await api.create("root", "sale_order_line", {
      order_id: order.id,
      description: "Travel",
      quantity: 1,
      unit_price_minor: 2500,
    });
    // The total is computed from the lines, and follows a line when it changes.
    expect((await api.run("root", "sale_order.get", { id: order.id })).total_minor).toBe(102500);
    await api.run("root", "sale_order_line.update", { id: line.id, quantity: 3 });
    expect((await api.run("root", "sale_order.get", { id: order.id })).total_minor).toBe(152500);

    await signIn("root", `/sales/sale_order?view=list&id=${order.id}`);
    const panel = panelOf(page, "Sale order");
    await expect(panel.getByLabel("Total")).toHaveValue(/1,?525\.00/);
    await expect(panel.getByLabel("Total")).toHaveAttribute("readonly", "");
    await panel.getByRole("button", { name: "Confirm", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "Confirm done" })).toBeVisible();

    const invoices = (await api.list("root", "invoice")).filter(
      (i) => i.sale_order_id === order.id,
    );
    expect(invoices).toHaveLength(1);
    expect(invoices[0].total_minor).toBe(152500);
    expect(invoices[0].state).toBe("draft");
    expect(invoices[0].number).toBeFalsy();

    await page.goto(`/#/invoicing/invoice?view=list&id=${invoices[0].id}`);
    const inv = panelOf(page, "Invoice");
    await expect(inv.getByLabel("Total")).toHaveValue(/1,?525\.00/);
    await inv.getByRole("button", { name: "Post *", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "Post done" })).toBeVisible();
    const posted = await api.run("root", "invoice.get", { id: invoices[0].id });
    expect(posted.state).toBe("posted");
    expect(posted.number).toMatch(/^INV-\d{4}-\d{5}$/);
    await expect(inv.getByLabel("Number")).toHaveValue(posted.number);
    await expect(inv.getByLabel("Number")).toHaveAttribute("readonly", "");
  });

  test("a second posted invoice gets the next number of the series", async ({ api, uid }) => {
    const numbers: number[] = [];
    for (let i = 0; i < 2; i++) {
      const party = await api.create("root", "party", { name: uid("Series customer") });
      const order = await api.create("root", "sale_order", { party_id: party.id });
      await api.create("root", "sale_order_line", {
        order_id: order.id,
        quantity: 1,
        unit_price_minor: 100,
      });
      await api.run("root", "sale_order.confirm", { id: order.id });
      const inv = (await api.list("root", "invoice")).find((x) => x.sale_order_id === order.id);
      await api.run("root", "invoice.post", { id: inv.id });
      const posted = await api.run("root", "invoice.get", { id: inv.id });
      numbers.push(Number(String(posted.number).split("-").pop()));
    }
    expect(numbers[1]).toBe(numbers[0]! + 1);
  });

  test("a duplicate unique value is refused with a clear inline message", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const sku = uid("SKU");
    await api.create("root", "product", { name: uid("First"), sku });
    await signIn("root", "/product/product?view=list&id=new");
    const panel = panelOf(page, "Product");
    await panel.getByLabel("Name").fill(uid("Second"));
    await panel.getByLabel("Sku").fill(sku);
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(panel.getByLabel("Sku")).toHaveAttribute("aria-invalid", "true");
    await expect(panel.getByRole("alert").first()).toContainText("must be unique");
    await expect(panel.getByRole("alert").first()).toContainText(sku);
  });

  test("an employee sees only her own leave requests", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      const employee = await api.create("root", "hr_employee", { name: uid("Staff") });
      const mine = await api.create("eve", "hr_leave_request", {
        employee_id: employee.id,
        leave_kind: "paid",
        start_on: "2026-10-01",
        end_on: "2026-10-02",
        days: 2,
      });
      expect(mine.requested_by).toBe("eve");
      const theirs = await api.create("root", "hr_leave_request", {
        employee_id: employee.id,
        leave_kind: "paid",
        start_on: "2026-11-01",
        end_on: "2026-11-03",
        days: 3,
        requested_by: "somebody-else",
      });
      const seen = (await api.list("eve", "hr_leave_request")).map((r) => r.id);
      expect(seen).toContain(mine.id);
      expect(seen).not.toContain(theirs.id);
      const forbidden = await api.post(
        "/api/hr_leave_request/get",
        { id: theirs.id },
        await api.token("eve"),
      );
      expect(forbidden.status).toBe(400);
      expect(forbidden.json.error).toBe(`no hr_leave_request ${theirs.id}`);
      await signIn("eve", "/hr/hr_leave_request?view=list");
      const grid = page.getByRole("grid", { name: "Leave request list" });
      await expect(grid.getByRole("row", { name: /Oct 1, 2026/ })).toBeVisible();
      await expect(grid.getByRole("row", { name: /Nov 1, 2026/ })).toHaveCount(0);
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });
});

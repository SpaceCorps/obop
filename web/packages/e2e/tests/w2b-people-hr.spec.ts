import { expect, test } from "../support/fixtures.ts";

// Employees (wave 2 stage B, UI plan 3.6): the org chart on the employee's manager, and a leave
// request filed on a phone by the employee herself, which starts on her own employee record
// (`hr_employee.user`). Leave that HR files for a linked employee belongs to that employee (B27):
// `requested_by` is filled from the employee's user by the server.

test.describe("employees", () => {
  test("the org chart shows who reports to whom and opens a person", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      const boss = await api.create("root", "hr_employee", {
        name: uid("Greta Boss"),
        job_title: "Managing Director",
      });
      const lead = await api.create("root", "hr_employee", {
        name: uid("Lars Lead"),
        job_title: "Warehouse Lead",
        reports_to_id: boss.id,
      });
      const clerk = await api.create("root", "hr_employee", {
        name: uid("Cleo Clerk"),
        job_title: "Stock Clerk",
        reports_to_id: lead.id,
      });

      await signIn("root", "/hr/org_chart");
      await expect(page.getByRole("heading", { name: "Org chart" })).toBeVisible();
      const tree = page.getByRole("tree", { name: "Org chart" });
      const bossItem = tree.getByRole("treeitem").filter({ hasText: boss.name }).first();
      await expect(bossItem).toBeVisible();
      // Two levels open at first: the boss's reports and theirs.
      await expect(tree.getByRole("link", { name: new RegExp(lead.name) })).toBeVisible();
      await expect(tree.getByRole("link", { name: new RegExp(clerk.name) })).toBeVisible();
      // Folding the boss hides everyone below; the count says how many.
      await bossItem.getByRole("button", { name: `Hide the 2 people below ${boss.name}` }).click();
      await expect(tree.getByRole("link", { name: new RegExp(clerk.name) })).toHaveCount(0);
      // A search opens the way to the person it finds.
      await page.getByRole("searchbox", { name: "Find a person or a job" }).fill("Stock Clerk");
      const hit = tree.getByRole("link", { name: new RegExp(clerk.name) });
      await expect(hit).toBeVisible();
      await hit.click();
      await expect(page).toHaveURL(new RegExp(`hr_employee\\?id=${clerk.id}`));
      await expect(page.getByRole("complementary", { name: /record$/ })).toContainText(clerk.name);
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });

  test("an employee asks for time off on a phone, starting on her own record", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      const eve = await api.create("root", "hr_employee", { name: uid("Eve Example"), user: "eve" });
      await api.create("root", "hr_employee", { name: uid("Someone Else") });

      // HR files leave for her: it is hers, not HR's (B27).
      const byHr = await api.create("root", "hr_leave_request", {
        employee_id: eve.id,
        leave_kind: "paid",
        start_on: "2026-12-01",
        end_on: "2026-12-01",
        days: 1,
      });
      expect(byHr.requested_by).toBe("eve");

      await page.setViewportSize({ width: 390, height: 844 });
      await signIn("eve", "/hr/hr_leave_request?view=list");
      await page.getByRole("button", { name: "New", exact: true }).click();
      // On a phone the record is a sheet over the list.
      const record = page.getByRole("dialog", { name: "Leave request record" });
      // The Employee field already holds her own record.
      await expect(record.getByRole("combobox", { name: "Employee" })).toHaveValue(eve.name);
      await record.getByLabel("First day").fill("2026-12-21");
      await record.getByLabel("Last day").fill("2026-12-23");
      await record.getByLabel("Working days").fill("3");
      await record.getByRole("button", { name: "Create" }).click();
      // Saved as a draft; the one big button sends it.
      await expect(record.getByText("Dec 21 – Dec 23")).toBeVisible();
      await record.getByRole("button", { name: /^Request/ }).click();
      await expect(record.getByText("Requested", { exact: true }).first()).toBeVisible();

      const mine = (await api.list("eve", "hr_leave_request")).filter(
        (r) => r.employee_id === eve.id,
      );
      expect(mine.map((r) => r.start_on).sort()).toEqual(["2026-12-01", "2026-12-21"]);
      const asked = mine.find((r) => r.start_on === "2026-12-21");
      expect(asked?.state).toBe("requested");
      expect(asked?.requested_by).toBe("eve");
      // No sideways scroll on the phone.
      const sideways = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(sideways).toBe(0);
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });
});

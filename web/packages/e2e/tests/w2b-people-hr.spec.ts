import { expect, test } from "../support/fixtures.ts";

// Employees (wave 2 stage B, UI plan 3.6): the org chart on the employee's manager, and a leave
// request filed on a phone by the employee herself, which starts on her own employee record
// (`hr_employee.user`). Leave that HR files for a linked employee belongs to that employee (B27):
// `requested_by` is filled from the employee's user by the server.

test.describe("employees", () => {
  test("a day picked in the leave calendar keeps its date and the person's own record", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      for (const was of (await api.list("root", "hr_employee")).filter((r) => r.user === "eve"))
        await api.post("/api/hr_employee/update", { id: was.id, user: null }, root);
      const eve = await api.create("root", "hr_employee", { name: uid("Eve Day"), user: "eve" });
      await signIn("eve", "/hr/hr_leave_request?view=calendar");
      // R2: the new form opened from a day starts on that day and on her own employee record.
      const day = page.getByRole("button", { name: /^New record on / }).nth(10);
      await day.click();
      const record = page.getByRole("dialog", { name: "Leave request record" });
      await expect(record.getByRole("combobox", { name: "Employee" })).toHaveValue(eve.name);
      await expect(record.getByLabel("First day")).not.toHaveValue("");
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });

  test("the time off list opens without refused leave until the filter is removed", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      const who = await api.create("root", "hr_employee", { name: uid("Rita Refused") });
      const leave = (start: string) =>
        api.create("root", "hr_leave_request", {
          employee_id: who.id,
          leave_kind: "paid",
          start_on: start,
          end_on: start,
          days: 1,
        });
      const kept = await leave("2026-12-02");
      const refused = await leave("2026-12-03");
      for (const a of ["request", "refuse"])
        await api.post(`/api/hr_leave_request/${a}`, { id: refused.id }, root);
      await signIn("root", "/hr/hr_leave_request?view=list");
      // R5: the package's default filter is on, and hides the refused request.
      const chip = page.getByRole("button", { name: /^Remove filter/ });
      await expect(chip).toHaveCount(1);
      const rows = page.getByRole("row").filter({ hasText: who.name });
      await expect(rows).toHaveCount(1);
      expect(kept.id).toBeTruthy();
      await chip.click();
      await expect(rows).toHaveCount(2);
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });

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
      const picker = await api.create("root", "hr_employee", {
        name: uid("Pia Picker"),
        job_title: "Picker",
        reports_to_id: lead.id,
      });

      await signIn("root", "/hr/org_chart");
      // The chart comes after the overview and the lists of the app, not above them.
      const side = page.getByRole("navigation", { name: /menu$/ });
      await expect(side.getByRole("link", { name: "Org chart" })).toBeVisible();
      const entries = await side.getByRole("link").allTextContents();
      expect(entries.indexOf("Overview")).toBeGreaterThanOrEqual(0);
      expect(entries.indexOf("Org chart")).toBeGreaterThan(entries.indexOf("Overview"));
      await expect(page.getByRole("heading", { name: "Org chart" })).toBeVisible();
      const tree = page.getByRole("tree", { name: "Org chart" });
      const bossItem = tree.getByRole("treeitem").filter({ hasText: boss.name }).first();
      await expect(bossItem).toBeVisible();
      // Two levels open at first: the boss's reports and theirs.
      await expect(tree.getByRole("link", { name: new RegExp(lead.name) })).toBeVisible();
      await expect(tree.getByRole("link", { name: new RegExp(clerk.name) })).toBeVisible();
      // Folding the boss hides everyone below; the count says how many.
      await bossItem.getByRole("button", { name: `Hide the 3 people below ${boss.name}` }).click();
      await expect(tree.getByRole("link", { name: new RegExp(clerk.name) })).toHaveCount(0);
      // A search narrows the chart to the people it finds and their managers.
      await page.getByRole("searchbox", { name: "Find a person or a job" }).fill("Stock Clerk");
      await expect(page.getByRole("status").filter({ hasText: "1 person matches" })).toBeVisible();
      const hit = tree.getByRole("link", { name: new RegExp(clerk.name) });
      await expect(hit).toBeVisible();
      await expect(tree.getByRole("link", { name: new RegExp(lead.name) })).toBeVisible();
      await expect(tree.getByRole("link", { name: new RegExp(picker.name) })).toHaveCount(0);
      await hit.click();
      await expect(page).toHaveURL(new RegExp(`hr_employee\\?id=${clerk.id}`));
      await expect(page.getByRole("complementary", { name: /record$/ })).toContainText(clerk.name);
    } finally {
      await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });

  test("a deep reporting line keeps its names readable at 320 px", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    await api.post("/api/marketplace_listing/install", { id: "hr" }, root);
    try {
      // Ten levels, one under the other.
      let above: string | undefined;
      let deepest = "";
      for (let level = 0; level < 10; level++) {
        const e = await api.create("root", "hr_employee", {
          name: uid(`Deep ${level}`),
          ...(above ? { reports_to_id: above } : {}),
        });
        above = e.id;
        deepest = e.name as string;
      }
      await page.setViewportSize({ width: 320, height: 720 });
      await signIn("root", "/hr/org_chart");
      await page.getByRole("searchbox", { name: "Find a person or a job" }).fill(deepest);
      const tree = page.getByRole("tree", { name: "Org chart" });
      const hit = tree.getByRole("link", { name: new RegExp(deepest) });
      await expect(hit).toBeVisible();
      const node = page.locator(".org-node.hit").first();
      const width = (await node.boundingBox())!.width;
      expect(width).toBeGreaterThanOrEqual(170);
      // The whole name shows: not cut by an ellipsis.
      const cut = await node
        .locator(".org-name")
        .evaluate((el) => el.scrollWidth > el.clientWidth || el.scrollHeight > el.clientHeight + 1);
      expect(cut).toBe(false);
      const sideways = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(sideways).toBe(0);
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
      // One login belongs to one employee (a run before this one may have linked "eve").
      for (const was of (await api.list("root", "hr_employee")).filter((r) => r.user === "eve"))
        await api.post("/api/hr_employee/update", { id: was.id, user: null }, root);
      const eve = await api.create("root", "hr_employee", {
        name: uid("Eve Example"),
        user: "eve",
      });
      const twice = await api.post(
        "/api/hr_employee/create",
        { name: uid("Eve Copy"), user: "eve" },
        root,
      );
      expect(twice.status, JSON.stringify(twice.json)).toBe(400);
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

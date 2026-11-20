import { expect, test, type Api } from "../support/fixtures.ts";

// Stream m6 (#102, S7): a step with no way back asks first though its package declares no
// confirm, a reconciled bank transaction can be taken back by a manager (accounting 0.6.0
// `unreconcile`), and the Everyone mode of the Timesheets grid names a worker by employee name.

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  const added: string[] = [];
  try {
    for (const id of ids)
      if (!had.has(id)) {
        await api.run("root", "marketplace_listing.install", { id });
        added.push(id);
      }
    await run();
  } finally {
    for (const id of added.reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

const pad = (n: number) => String(n).padStart(2, "0");
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
function monday(): string {
  const d = new Date();
  d.setHours(12, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return iso(d);
}

test.describe("a step with no way back (#102)", () => {
  test.setTimeout(90_000);

  test("asks before it runs, from the button of the record", async ({ page, signIn, api, uid }) => {
    await withPackages(api, ["hr", "planning"], async () => {
      const person = await api.create("root", "hr_employee", { name: uid("One Way") });
      const shift = await api.create("root", "planning_shift", {
        name: uid("Late shift"),
        work_date: monday(),
        start_minute: 600,
        end_minute: 720,
        employee_id: person.id,
      });
      await api.run("root", "planning_shift.publish", { id: shift.id });

      await signIn("root", `/planning/planning_shift?id=${shift.id}`);
      await page.locator("aside").getByRole("button", { name: "Confirm", exact: true }).click();
      const ask = page.getByRole("alertdialog");
      await expect(ask).toContainText("There is no way back");
      await ask.getByRole("button", { name: "Cancel" }).click();
      expect((await api.run("root", "planning_shift.get", { id: shift.id })).state).toBe(
        "published",
      );

      await page.locator("aside").getByRole("button", { name: "Confirm", exact: true }).click();
      await page.getByRole("alertdialog").getByRole("button", { name: "Confirm" }).click();
      await expect
        .poll(async () => (await api.run("root", "planning_shift.get", { id: shift.id })).state)
        .toBe("confirmed");
    });
  });

  test("a manager takes back a reconciliation", async ({ page, signIn, api, uid }) => {
    await withPackages(api, ["accounting"], async () => {
      const tag = uid("U").replace(/\s+/g, "");
      const bank = await api.create("root", "accounting_account", {
        code: `B${tag}`,
        name: uid("Bank"),
        account_type: "asset_cash",
      });
      const income = await api.create("root", "accounting_account", {
        code: `I${tag}`,
        name: uid("Other income"),
        account_type: "income_other",
      });
      const journal = await api.create("root", "accounting_journal", {
        name: uid("Bank journal"),
        code: `BJ${tag}`,
        kind: "bank",
        default_account_id: bank.id,
      });
      const line = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-03",
        label: uid("Interest"),
        amount: 1200,
        state: "unmatched",
      });
      const booked = await api.run("root", "bank_matching.book", {
        bank_line: line.id,
        counter_account: income.id,
      });
      const items = await api.run("root", "accounting_move_line.search", {
        filter: [{ field: "move_id", op: "eq", value: booked.move_id }],
      });
      await api.run("root", "accounting_bank_statement_line.update", {
        id: line.id,
        move_line_id: items.rows[0].id,
      });
      await api.run("root", "accounting_bank_statement_line.match", { id: line.id });
      const get = async () =>
        (await api.run("root", "accounting_bank_statement_line.get", { id: line.id })).state;
      expect(await get()).toBe("matched");
      await api.run("root", "accounting_bank_statement_line.reconcile", { id: line.id });

      await signIn("root", `/accounting/accounting_bank_statement_line?id=${line.id}`);
      await page.getByRole("button", { name: "More actions" }).click();
      await page.getByRole("menuitem", { name: /Undo reconciliation/ }).click();
      const ask = page.getByRole("alertdialog");
      await expect(ask).toContainText("Undo the reconciliation");
      await ask.getByRole("button", { name: /Undo reconciliation/ }).click();
      await expect(page.getByText("Reconciliation undone")).toBeVisible();
      expect(await get()).toBe("matched");
    });
  });
});

test.describe("the Timesheets grid in Everyone mode (S7)", () => {
  test.setTimeout(90_000);

  test("names the worker by the employee linked to the login", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, ["hr", "projects", "timesheets"], async () => {
      const linked = (
        (await api.run("root", "hr_employee.search", {
          filter: [{ field: "user", op: "eq", value: "root" }],
        })) as { rows: any[] }
      ).rows[0];
      const made = linked
        ? null
        : await api.create("root", "hr_employee", { name: uid("Grid Worker"), user: "root" });
      const name: string = linked?.name ?? made.name;
      try {
        const project = await api.create("root", "projects_project", { name: uid("Grid") });
        const task = await api.create("root", "projects_task", {
          title: uid("Named line"),
          project_id: project.id,
        });
        await api.create("root", "timesheets_entry", {
          task_id: task.id,
          worked_on: monday(),
          minutes: 60,
        });
        await signIn("root", "/timesheets/timesheets_entry");
        await page.getByRole("radio", { name: "Everyone" }).click();
        const row = page.getByRole("row", { name: new RegExp(task.title) });
        await expect(row.locator(".tsgrid-owner")).toHaveText(name);
      } finally {
        if (made) await api.run("root", "hr_employee.update", { id: made.id, user: "" });
      }
    });
  });
});

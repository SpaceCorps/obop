import { expect, test, type Api } from "../support/fixtures.ts";

// Planning (wave 2, field): the weekly shift plan. Shifts open on the week plan, a row per employee
// and one for open shifts; a cell starts a new shift on its employee and day; "Publish week" runs
// `planning_shift.publish` on each shift of the week that is not published. On a phone the week is a
// list by day.

const pad = (n: number) => String(n).padStart(2, "0");
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
/** A day of this week, Monday + n. */
function weekDay(n: number): string {
  const d = new Date();
  d.setHours(12, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7) + n);
  return iso(d);
}

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

test.describe("Planning: the week plan", () => {
  test.setTimeout(90_000);

  test("plan, start a shift from a cell and publish the week", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await withPackages(api, ["hr", "planning"], async () => {
      const person = await api.create("root", "hr_employee", { name: uid("Nina Petersen") });
      const morning = await api.create("root", "planning_shift", {
        name: uid("Morning counter"),
        work_date: weekDay(1),
        start_minute: 420,
        end_minute: 900,
        employee_id: person.id,
      });
      const open = await api.create("root", "planning_shift", {
        name: uid("Saturday store"),
        work_date: weekDay(5),
        start_minute: 480,
        end_minute: 780,
      });

      await signIn("root", "/planning");
      await expect(page.getByRole("heading", { name: "Week plan" })).toBeVisible();
      const row = page.getByRole("row", { name: new RegExp(person.name) });
      await expect(row).toContainText("07:00–15:00");
      await expect(row).toContainText("Not published");
      const openRow = page.getByRole("row", { name: /^Open shifts/ });
      await expect(openRow).toContainText(open.name);

      // A cell starts a new shift on its employee and day.
      await row
        .getByRole("button", { name: new RegExp(`Add a shift for ${person.name}`) })
        .first()
        .click();
      await expect(page.getByRole("combobox", { name: "Employee" })).toHaveValue(person.name);
      await page.goBack();
      await expect(page.getByRole("heading", { name: "Week plan" })).toBeVisible();

      // The week being planned comes back after a shift was opened and closed.
      const range = page.locator(".pl-range");
      const thisWeek = await range.innerText();
      await page.getByRole("button", { name: "Next week" }).click();
      await expect(range).not.toHaveText(thisWeek);
      const nextWeek = await range.innerText();
      await page
        .getByRole("button", { name: /^Add a shift for / })
        .first()
        .click();
      await expect(page.getByRole("combobox", { name: "Employee" })).toBeVisible();
      await page.goBack();
      await expect(range).toHaveText(nextWeek);
      await page.getByRole("button", { name: "This week" }).click();
      await expect(range).toHaveText(thisWeek);

      // Publish week: one publish per shift, each a command.
      await page.getByRole("button", { name: /^Publish week \(\d+\)$/ }).click();
      // Publishing is not destructive: the question's button is the normal primary one.
      const publish = page.getByRole("alertdialog").getByRole("button", { name: "Publish" });
      await expect(publish).toHaveClass(/\bprimary\b/);
      await expect(publish).not.toHaveClass(/\bdanger\b/);
      await publish.click();
      await expect(page.getByRole("button", { name: "Week published" })).toBeVisible();
      for (const s of [morning, open])
        expect((await api.run("root", "planning_shift.get", { id: s.id })).state).toBe("published");

      // On a phone the week is a list by day, with no sideways scroll.
      await page.setViewportSize({ width: 390, height: 844 });
      await expect(page.locator(".pl-days")).toContainText(morning.name);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(0);
    });
  });
});

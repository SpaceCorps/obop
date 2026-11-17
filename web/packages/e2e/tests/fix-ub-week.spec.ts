import { expect, test, type Api } from "../support/fixtures.ts";

// The two week views (docs/ISSUES.md S6, S7, S8 and R18): the week plan keeps its week in the
// address, so a reload opens the same week; a long shift name takes two lines in its cell; a line
// of the Timesheets grid names its project under the task; with touch a locked cell is as large as
// a typed one; and at 320 px the strip of days scrolls inside itself with 44 px days.

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

test.describe("the week plan", () => {
  test.setTimeout(90_000);

  test("keeps its week through a reload, and a long shift name takes two lines (S6, R18)", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, ["hr", "planning"], async () => {
      const person = await api.create("root", "hr_employee", { name: uid("Week Keeper") });
      const long = await api.create("root", "planning_shift", {
        name: uid("Workshop: double glazing units for the north wing, second fitting"),
        work_date: weekDay(7 + 2),
        start_minute: 420,
        end_minute: 900,
        employee_id: person.id,
      });

      await signIn("root", "/planning");
      await expect(page.getByRole("heading", { name: "Week plan" })).toBeVisible();
      await page.getByRole("button", { name: "Next week" }).click();
      await expect(page).toHaveURL(new RegExp(`week=${weekDay(7)}`));
      const name = page.locator(".pl-shift-name", { hasText: long.name });
      await expect(name).toBeVisible();

      await page.reload();
      await expect(page.getByRole("heading", { name: "Week plan" })).toBeVisible();
      await expect(name).toBeVisible();

      // Two lines in the cell, not one cut line.
      const lines = await name.evaluate((e) => {
        const s = getComputedStyle(e);
        return Math.round(e.getBoundingClientRect().height / parseFloat(s.lineHeight));
      });
      expect(lines).toBe(2);
    });
  });
});

async function timesheetsWithLockedEntry(api: Api, uid: (s: string) => string) {
  const project = await api.create("root", "projects_project", { name: uid("Grid parent") });
  const task = await api.create("root", "projects_task", {
    title: uid("Grid line"),
    project_id: project.id,
  });
  const entry = await api.create("root", "timesheets_entry", {
    task_id: task.id,
    worked_on: weekDay(0),
    minutes: 90,
  });
  await api.run("root", "timesheets_entry.submit", { id: entry.id });
  return { project, task };
}

test.describe("the Timesheets week grid", () => {
  test.setTimeout(90_000);

  test("names the project of a task under it (S7)", async ({ page, signIn, api, uid }) => {
    await withPackages(api, ["projects", "timesheets"], async () => {
      const { project, task } = await timesheetsWithLockedEntry(api, uid);
      await signIn("root", "/timesheets/timesheets_entry");
      const row = page.getByRole("row", { name: new RegExp(task.title) });
      await expect(row.locator(".tsgrid-parent")).toHaveText(project.name);
    });
  });

  test.describe("with touch", () => {
    test.use({ viewport: { width: 834, height: 1112 }, hasTouch: true });

    test("a locked cell is as large as a typed one (S8)", async ({ page, signIn, api, uid }) => {
      await withPackages(api, ["projects", "timesheets"], async () => {
        await timesheetsWithLockedEntry(api, uid);
        await signIn("root", "/timesheets/timesheets_entry");
        const locked = page.locator(".tsgrid-locked").first();
        const typed = page.locator(".tsgrid-input").first();
        await expect(locked).toBeVisible();
        await expect(typed).toBeVisible();
        const size = (l: typeof locked) => l.evaluate((e) => getComputedStyle(e).fontSize);
        expect(await size(locked)).toBe(await size(typed));
      });
    });
  });

  test.describe("at 320 px", () => {
    test.use({ viewport: { width: 320, height: 700 }, hasTouch: true, isMobile: true });

    test("the days are 44 px and scroll inside the strip (R18)", async ({
      page,
      signIn,
      api,
      uid,
    }) => {
      await withPackages(api, ["projects", "timesheets"], async () => {
        await timesheetsWithLockedEntry(api, uid);
        await signIn("root", "/timesheets/timesheets_entry");
        const days = page.getByRole("tab");
        await expect(days).toHaveCount(7);
        for (let i = 0; i < 7; i++) {
          const box = await days.nth(i).evaluate((e) => e.getBoundingClientRect().width);
          expect(box, `day ${i}`).toBeGreaterThanOrEqual(44);
        }
        const strip = page.getByRole("tablist", { name: "Days of the week" });
        expect(await strip.evaluate((e) => e.scrollWidth > e.clientWidth)).toBe(true);
        expect(
          await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),
        ).toBe(false);
        // The chosen day (today) is in view inside the strip.
        const picked = page.getByRole("tab", { selected: true });
        const inView = await picked.evaluate((e) => {
          const s = e.parentElement!.getBoundingClientRect();
          const r = e.getBoundingClientRect();
          return r.left >= s.left - 1 && r.right <= s.right + 1;
        });
        expect(inView).toBe(true);
      });
    });
  });
});

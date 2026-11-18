import { expect, test, type Api } from "../support/fixtures.ts";

// The calendar (docs/ISSUES.md U13 and S5): a leave started from a day of the calendar ends that
// day too, so its range never ends before it starts; and the week's time grid writes its hours and
// its events on the 24-hour clock the time boxes and the lists use, in an English browser too.

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

test("a leave made from a calendar day starts and ends that day (U13)", async ({
  page,
  signIn,
  api,
}) => {
  test.setTimeout(90_000);
  await withPackages(api, ["hr"], async () => {
    await signIn("root", "/hr/hr_leave_request?view=calendar");
    // The last day of the month shown: after today in most months, never today's own default.
    const days = page.getByRole("button", { name: /^New record on / });
    await expect(days.first()).toBeVisible();
    const day = days.nth((await days.count()) - 1);
    const label = (await day.getAttribute("aria-label"))!.replace("New record on ", "");
    const shown = new Date(label).toLocaleDateString("en-US", {
      month: "2-digit",
      day: "2-digit",
      year: "numeric",
    });
    await day.click();
    await expect(page.locator("input").and(page.getByLabel("First day"))).toHaveValue(shown);
    await expect(page.locator("input").and(page.getByLabel("Last day"))).toHaveValue(shown);
  });
});

test("the week's time grid reads the 24-hour clock in an English browser (S5)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  test.setTimeout(90_000);
  await withPackages(api, ["hr", "planning"], async () => {
    const today = new Date().toISOString().slice(0, 10);
    await api.create("root", "planning_shift", {
      name: uid("Afternoon shift"),
      work_date: today,
      start_minute: 870,
      end_minute: 1020,
    });
    await signIn("root", "/planning/planning_shift?view=calendar");
    await page.getByRole("button", { name: "Week", exact: true }).click();
    const grid = page.getByRole("group", { name: "Week", exact: true });
    await expect(grid).toBeVisible();
    await expect(grid).toContainText("14:30");
    await expect(grid).toContainText("15:00");
    await expect(grid).not.toContainText(/\d (AM|PM)/);
  });
});

import { expect, test, type Api } from "../support/fixtures.ts";

// Timesheets (wave 2, services, plan 3.5): time entries open on a week grid. Typing hours into an
// empty cell creates an entry, changing them updates it, and "Submit week" submits the week's
// drafts, one `timesheets_entry.submit` each. Every step is a command, so the API sees it all.

async function setUp(api: Api, uid: (s: string) => string) {
  const root = await api.token("root");
  for (const id of ["projects", "timesheets"]) {
    const r = await api.post("/api/marketplace_listing/install", { id }, root);
    expect([200, 409]).toContain(r.status);
  }
  const project = await api.create("root", "projects_project", { name: uid("Grid project") });
  const task = await api.create("root", "projects_task", {
    title: uid("Grid task"),
    project_id: project.id,
  });
  return { root, task };
}

async function tearDown(api: Api, root: string) {
  await api.post("/api/module/uninstall", { id: "timesheets" }, root);
  await api.post("/api/module/uninstall", { id: "projects" }, root);
}

const mine = async (api: Api, task: string) =>
  (await api.list("root", "timesheets_entry")).filter((r) => r.task_id === task);

test("a week of time is typed into the grid and submitted for approval", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const { root, task } = await setUp(api, uid);
  try {
    await signIn("root", "/timesheets/timesheets_entry");
    await expect(page.getByRole("button", { name: "This week" })).toBeVisible();

    // A line for the task, then 1:30 on its first day.
    await page.getByPlaceholder(/Search task/).fill(task.title);
    await page.getByRole("option", { name: new RegExp(task.title) }).click();
    const cells = page.getByRole("textbox", { name: new RegExp(`^${task.title} on `) });
    await expect(cells).toHaveCount(7);
    await cells.first().fill("1:30");
    await cells.first().press("Enter");
    await expect.poll(async () => (await mine(api, task.id)).map((r) => r.minutes)).toEqual([90]);
    await expect(cells.first()).toHaveValue("1 h 30");

    // Changing the hours updates the same entry; the line total follows.
    await cells.first().fill("2.5h");
    await cells.first().press("Enter");
    await expect.poll(async () => (await mine(api, task.id)).map((r) => r.minutes)).toEqual([150]);
    await expect(page.getByRole("row", { name: new RegExp(task.title) })).toContainText("2 h 30");

    // Submit week: the draft is submitted and its cell is no longer typed over.
    await page.getByRole("button", { name: /^Submit week \(\d+\)$/ }).click();
    await expect.poll(async () => (await mine(api, task.id)).map((r) => r.state)).toEqual([
      "submitted",
    ]);
    await expect(
      page.getByRole("button", { name: new RegExp(`^${task.title} on .*: 2 h 30$`) }),
    ).toBeVisible();
    await expect(cells).toHaveCount(6);
  } finally {
    await tearDown(api, root);
  }
});

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  test("a day is picked from the strip and its hours typed in one large box", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const { root, task } = await setUp(api, uid);
    try {
      await signIn("root", "/timesheets/timesheets_entry");
      const days = page.getByRole("tab");
      await expect(days).toHaveCount(7);
      await page.getByPlaceholder(/Search task/).fill(task.title);
      await page.getByRole("option", { name: new RegExp(task.title) }).click();
      await days.nth(1).click();
      await expect(days.nth(1)).toHaveAttribute("aria-selected", "true");
      const box = page.getByRole("textbox", { name: new RegExp(`^${task.title} on `) });
      await expect(box).toHaveCount(1);
      const height = (await box.boundingBox())?.height ?? 0;
      expect(height).toBeGreaterThanOrEqual(44);
      await box.fill("45");
      await box.press("Enter");
      await expect.poll(async () => (await mine(api, task.id)).map((r) => r.minutes)).toEqual([45]);
      await expect(days.nth(1)).toContainText("45 min");
      const sideways = await page.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      );
      expect(sideways).toBe(0);
    } finally {
      await tearDown(api, root);
    }
  });
});

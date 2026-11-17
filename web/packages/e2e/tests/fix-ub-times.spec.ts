import { expect, test, type Api } from "../support/fixtures.ts";

// One way to write a time of day (docs/ISSUES.md S5): the time boxes of a form read and write the
// 24-hour clock the lists, My day and the week plan show ("07:00"), in an English browser too
// (the suite's locale is en-US), instead of the browser's own "07:00 AM" time box.

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

test("a shift's time boxes read and take the 24-hour clock (S5)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  test.setTimeout(90_000);
  await withPackages(api, ["hr", "planning"], async () => {
    const shift = await api.create("root", "planning_shift", {
      name: uid("Clock shift"),
      work_date: new Date().toISOString().slice(0, 10),
      start_minute: 420,
      end_minute: 930,
    });
    await signIn("root", `/planning/planning_shift?view=list&id=${shift.id}`);
    const record = page.getByRole("complementary", { name: /record$/ });
    const start = record.getByRole("textbox", { name: "Start" });
    const end = record.getByRole("textbox", { name: "End" });
    // A text box: the browser's time box would draw "07:00 AM" in an English browser.
    await expect(start).toHaveAttribute("type", "text");
    await expect(start).toHaveValue("07:00");
    await expect(end).toHaveValue("15:30");

    // Typed as on a keypad, saved as minutes after midnight.
    await start.fill("1430");
    await start.blur();
    await expect(start).toHaveValue("14:30");
    await end.fill("17.15");
    await end.blur();
    await record.getByRole("button", { name: "Save" }).click();
    await expect
      .poll(async () => {
        const got = await api.run("root", "planning_shift.get", { id: shift.id });
        return [got.start_minute, got.end_minute];
      })
      .toEqual([870, 1035]);
  });
});

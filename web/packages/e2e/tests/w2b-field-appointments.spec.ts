import { expect, test, type Api } from "../support/fixtures.ts";

// Appointments (wave 2, field): booking a slot on a phone. The form shows the free half-hour slots
// of the chosen resource on the chosen day (from `appointments_appointment.search`); a booked slot
// cannot be picked, a free one fills Start and End, and a time that overlaps another booking is
// named before saving. Create is the generated `appointments_appointment.create`.

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

test.describe("Appointments: book a free slot", () => {
  test.setTimeout(90_000);

  test("on a phone: pick a resource, see the free times, book one", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await withPackages(api, ["appointments"], async () => {
      const room = await api.create("root", "appointments_resource", {
        name: uid("Meeting room Elbe"),
        kind: "room",
      });
      // The form starts on today; book 10:00 to 11:00 of today already.
      await signIn("root", "/appointments/appointments_appointment?id=new");
      const now = new Date();
      const day = [now.getFullYear(), now.getMonth() + 1, now.getDate()]
        .map((n) => String(n).padStart(2, "0"))
        .join("-");
      await api.create("root", "appointments_appointment", {
        title: uid("Supplier visit"),
        resource_id: room.id,
        day,
        start_minute: 600,
        end_minute: 660,
        attendee_name: "Max Weber",
      });

      const title = uid("Repair intake");
      await page.getByRole("textbox", { name: "Title" }).fill(title);
      await page.getByRole("textbox", { name: "Attendee", exact: true }).fill("Lena Busch");
      const res = page.getByRole("combobox", { name: "Resource" });
      await res.click();
      await res.fill(room.name);
      await page
        .getByRole("listbox", { name: "Resource options" })
        .getByRole("option", { name: new RegExp(room.name) })
        .click();

      const slots = page.getByRole("group", { name: "Free times" });
      await expect(slots.getByRole("button", { name: "10:00" })).toBeDisabled();
      await expect(slots.getByRole("button", { name: "10:30" })).toBeDisabled();
      await expect(slots.getByRole("button", { name: "09:30" })).toBeEnabled();

      // A time that overlaps the booking is named before saving.
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("10:15");
      await page.getByRole("textbox", { name: "End", exact: true }).fill("10:45");
      await expect(page.getByRole("alert").filter({ hasText: "Clashes with" })).toBeVisible();

      // A free slot fills Start and End and the clash is gone.
      await slots.getByRole("button", { name: "11:00" }).click();
      await expect(page.getByRole("textbox", { name: "Start", exact: true })).toHaveValue("11:00");
      await expect(page.getByRole("textbox", { name: "End", exact: true })).toHaveValue("11:30");
      await expect(page.getByRole("alert").filter({ hasText: "Clashes with" })).toHaveCount(0);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(0);

      await page.getByRole("button", { name: "Create", exact: true }).click();
      await expect
        .poll(async () => {
          const r = await api.run("root", "appointments_appointment.search", {
            filter: [{ field: "title", op: "eq", value: title }],
          });
          return r.rows.map((x: any) => [x.start_minute, x.end_minute, x.resource_id]);
        })
        .toEqual([[660, 690, room.id]]);
    });
  });
});

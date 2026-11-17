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
      // The form starts on today (UTC, the zone of the test server and browser); the browser's
      // clock stands at 07:00 of today so that every slot of the day is still ahead. Book 10:00
      // to 11:00 of today already.
      const now = new Date();
      const day = [now.getUTCFullYear(), now.getUTCMonth() + 1, now.getUTCDate()]
        .map((n) => String(n).padStart(2, "0"))
        .join("-");
      await page.clock.setFixedTime(new Date(`${day}T07:00:00Z`));
      await signIn("root", "/appointments/appointments_appointment?id=new");
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
      await expect(slots.getByRole("button", { name: "10:00 AM" })).toBeDisabled();
      await expect(slots.getByRole("button", { name: "10:30 AM" })).toBeDisabled();
      await expect(slots.getByRole("button", { name: "09:30 AM" })).toBeEnabled();

      // A time that overlaps the booking is named before saving.
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("10:15");
      await page.getByRole("textbox", { name: "End", exact: true }).fill("10:45");
      await expect(page.getByRole("alert").filter({ hasText: "Clashes with" })).toBeVisible();

      // With an hour chosen, 09:30 is free but too short (it runs into 10:00): dashed, not struck
      // through like the booked 10:00.
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("09:00");
      await page.getByRole("textbox", { name: "End", exact: true }).fill("10:00");
      await expect(slots.getByRole("button", { name: "09:30 AM" })).toHaveClass(/is-short/);
      await expect(slots.getByRole("button", { name: "10:00 AM" })).toHaveClass(/is-booked/);
      await expect(page.getByText(/Dashed: free, but too short for 60 minutes/)).toBeVisible();

      // Later in the day the slots that have begun are past: not offered, and said so.
      await page.clock.setFixedTime(new Date(`${day}T10:40:00Z`));
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("08:00");
      await expect(slots.getByRole("button", { name: "08:00 AM" })).toHaveClass(/is-past/);
      await expect(slots.getByRole("button", { name: "08:00 AM" })).toBeDisabled();
      await expect(slots.getByRole("button", { name: "10:30 AM" })).toHaveClass(/is-past/);
      await expect(slots.getByRole("button", { name: "11:00 AM" })).toBeEnabled();
      await expect(page.getByText("Earlier times today have passed.")).toBeVisible();
      // Late in the evening nothing is left today, and the line says so rather than "fully booked".
      await page.clock.setFixedTime(new Date(`${day}T23:00:00Z`));
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("09:00");
      await expect(page.getByText(/no free time left today/)).toBeVisible();
      await page.clock.setFixedTime(new Date(`${day}T10:40:00Z`));
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("08:00");
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("09:00");

      // A free slot fills Start and End and the clash is gone.
      await slots.getByRole("button", { name: "11:00 AM" }).click();
      await expect(page.getByRole("textbox", { name: "Start", exact: true })).toHaveValue("11:00");
      await expect(page.getByRole("textbox", { name: "End", exact: true })).toHaveValue("12:00");
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
        .toEqual([[660, 720, room.id]]);
      // The form reads the day's bookings again after the save; let that finish before the
      // package is uninstalled, or the late search is answered 404.
      await page.waitForLoadState("networkidle");
    });
  });
});

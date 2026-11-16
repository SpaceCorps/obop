import { expect, test, type Api } from "../support/fixtures.ts";

// Field Service (wave 2, field): a technician's day on a phone from start to finish. The app opens
// on My day with today's job, On my way, Start work, the customer signs on the job, Complete job,
// and the printed job report carries the sign-off. Every step is a command: the job's actions
// `depart`, `start` and `complete`, and `field_service_job.update` for the sign-off; the server
// refuses `complete` while a required sign-off is missing.

const pad = (n: number) => String(n).padStart(2, "0");
const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

/** Install the packages that are missing, run, and remove what was installed (last first). */
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

test.describe("Field Service: a technician's day on a phone", () => {
  test.setTimeout(90_000);

  test("My day, on my way, start, sign-off, complete and the report", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await withPackages(api, ["hr", "field_service"], async () => {
      // The signed-in user is a technician: an employee linked to the login.
      const linked = await api.run("root", "hr_employee.search", {
        filter: [{ field: "user", op: "eq", value: "root" }],
      });
      const tech =
        linked.rows[0] ??
        (await api.create("root", "hr_employee", { name: uid("Piotr Nowak"), user: "root" }));
      try {
        const customer = await api.create("root", "party", { name: uid("Hafen GmbH") });
        const name = uid("Replace the boiler pump");
        const job = await api.create("root", "field_service_job", {
          name,
          customer_id: customer.id,
          technician_id: tech.id,
          scheduled_on: today(),
          scheduled_start_minute: 540,
          planned_minutes: 90,
          signature_required: true,
          site_address: "Hafenstrasse 1",
          site_postcode: "20457",
          site_city: "Hamburg",
          site_contact_name: "Jens Moeller",
          site_contact_phone: "+49 40 555 0123",
        });

        // On a phone the app opens on My day.
        await signIn("root", "/field_service");
        await expect(page.getByRole("heading", { name: "My day" })).toBeVisible();
        const card = page.locator(".fs-job", { hasText: name });
        await expect(card).toContainText("09:00");
        await expect(card).toContainText("1 h 30");
        await expect(
          card.getByRole("link", { name: /Hafenstrasse 1, 20457 Hamburg/ }),
        ).toHaveAttribute("href", /openstreetmap/);
        await expect(card.getByRole("link", { name: "+49 40 555 0123" })).toHaveAttribute(
          "href",
          "tel:+49405550123",
        );
        // No sideways scroll on the phone.
        expect(
          await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
        ).toBeLessThanOrEqual(0);

        await card.getByRole("button", { name: "On my way" }).click();
        await expect(card).toContainText("On the way");
        await card.getByRole("button", { name: "Start work" }).click();
        await expect(card).toContainText("In progress");

        // The server holds the sign-off rule: complete is refused without it.
        const refused = await api.post(
          "/api/field_service_job/complete",
          { id: job.id },
          await api.token("root"),
        );
        expect(refused.status).toBe(400);
        expect(refused.json.assert_key).toBe("signed");

        // The customer signs on the job.
        await card.getByRole("button", { name: "Get signature" }).click();
        await page.getByRole("button", { name: "Customer signs" }).click();
        const dialog = page.getByRole("dialog", { name: "Sign off the work" });
        const signer = dialog.getByRole("textbox", { name: "Name of the person signing" });
        await expect(signer).toHaveValue("Jens Moeller");
        await dialog.getByRole("button", { name: "Sign" }).click();
        await expect(dialog).toBeHidden();
        await expect(page.getByRole("button", { name: "Customer signs" })).toHaveCount(0);

        // The printed report carries the sign-off.
        await expect(page.locator(".print-sheet .fs-print")).toContainText(
          "Signed by Jens Moeller",
        );

        // Back on My day the job can be completed.
        await signIn("root", "/field_service/my-day");
        const again = page.locator(".fs-job", { hasText: name });
        await again.getByRole("button", { name: "Complete job" }).click();
        await expect(again).toContainText("Done");
        const done = await api.run("root", "field_service_job.get", { id: job.id });
        expect(done.state).toBe("done");
        expect(done.signed_by).toBe("Jens Moeller");
        expect(done.signed_on).toBe(today());
      } finally {
        if (!linked.rows[0])
          await api.post(
            "/api/hr_employee/update",
            { id: tech.id, user: null },
            await api.token("root"),
          );
      }
    });
  });
});

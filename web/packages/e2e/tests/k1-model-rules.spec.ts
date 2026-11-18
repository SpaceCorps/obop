import { expect, test, type Api } from "../support/fixtures.ts";

// Model rules (D88, first slice), no overlap and field formats (K45) over the real server: a
// booking that overlaps another one of the same resource is refused even when the person saves
// it anyway, and the form says why in a sentence; a fiscal year that ends before it starts, or
// lies outside its days, and a journal whose currency is not a currency code are refused on REST.

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

test.describe("Model rules hold on the server", () => {
  test.setTimeout(120_000);

  test("on a phone: an overlapping booking is refused and the form says why", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    // The refused create is the 400 this test expects.
    allowHttpErrors(400);
    await page.setViewportSize({ width: 390, height: 844 });
    await withPackages(api, ["appointments"], async () => {
      const room = await api.create("root", "appointments_resource", {
        name: uid("Meeting room Isar"),
        kind: "room",
      });
      const now = new Date();
      const day = [now.getUTCFullYear(), now.getUTCMonth() + 1, now.getUTCDate()]
        .map((n) => String(n).padStart(2, "0"))
        .join("-");
      await page.clock.setFixedTime(new Date(`${day}T07:00:00Z`));
      await api.create("root", "appointments_appointment", {
        title: uid("Supplier visit"),
        resource_id: room.id,
        day,
        start_minute: 600,
        end_minute: 660,
        attendee_name: "Max Weber",
      });
      await signIn("root", "/appointments/appointments_appointment?id=new");
      const title = uid("Overlapping intake");
      await page.getByRole("textbox", { name: "Title" }).fill(title);
      await page.getByRole("textbox", { name: "Attendee", exact: true }).fill("Lena Busch");
      await page.getByRole("combobox", { name: "Resource" }).click();
      await page
        .getByRole("listbox", { name: "Resource options" })
        .getByRole("option", { name: new RegExp(room.name) })
        .click();
      await page.getByRole("textbox", { name: "Start", exact: true }).fill("10:15");
      await page.getByRole("textbox", { name: "End", exact: true }).fill("10:45");
      await page.getByRole("button", { name: "Create", exact: true }).click();

      const refusal = page
        .getByRole("alert")
        .filter({ hasText: "The resource is already booked at that time" });
      await expect(refusal).toBeVisible();
      // A sentence for a person: the engine's suffix is not shown.
      await expect(refusal).not.toContainText("rule no_double_booking");
      const stored = await api.run("root", "appointments_appointment.search", {
        filter: [{ field: "title", op: "eq", value: title }],
      });
      expect(stored.rows).toEqual([]);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(0);
    });
  });

  test("REST: a rule, a date bound and a currency code are refused with their codes", async ({
    api,
    uid,
  }) => {
    await withPackages(api, ["accounting"], async () => {
      const root = await api.token("root");
      const fy = (body: object) => api.post("/api/accounting_fiscal_year/create", body, root);

      const backwards = await fy({
        name: uid("FY"),
        start_on: "2031-01-01",
        end_on: "2030-12-31",
      });
      expect(backwards.status).toBe(400);
      expect(backwards.json.code).toBe("assertion_failed");
      expect(backwards.json.assert_key).toBe("ends_after_start");

      const ancient = await fy({ name: uid("FY"), start_on: "0313-12-13", end_on: "1333-03-31" });
      expect(ancient.status).toBe(400);
      expect(ancient.json.code).toBe("limit_violated");
      expect(ancient.json.error).toContain("must be at least 1900-01-01");

      const journal = await api.post(
        "/api/accounting_journal/create",
        { name: uid("Bank"), code: uid("B").slice(-8), kind: "bank", currency: "12y12" },
        root,
      );
      expect(journal.status).toBe(400);
      expect(journal.json.error).toContain("currency code");
    });
  });
});

import { expect, test } from "../support/fixtures.ts";
import { coreSeries } from "../support/env.ts";

// The record form of the UI plan (2.6, lane L5): an action saves what was typed before it runs
// (B4), the header names a record the way a person would, and a record with lines opens as a page.

const q = (s: string) => JSON.stringify(s);

/** An order with lines: `<id>_order` and its child `<id>_order_line`, linked by `order_id`. */
const withLines = (id: string) => `
[module]
id = ${q(id)}
version = "0.1.0"
name = "Form orders"
description = "An order with lines (end-to-end test module)"
author = "obop e2e"
requires_core = ${q(coreSeries)}

[[model]]
name = "${id}_order"
description = "An order"
read_group = "base.user"
write_group = "${id}.manager"

  [[model.field]]
  name = "name"
  description = "What the order is called"
  class = "none"
  required = true

[[model]]
name = "${id}_order_line"
description = "A line of an order"
read_group = "base.user"
write_group = "${id}.manager"

  [[model.field]]
  name = "order_id"
  description = "The order the line belongs to"
  class = "none"
  kind = "ref"
  to = "${id}_order"
  required = true

  [[model.field]]
  name = "description"
  description = "What the line is for"
  class = "none"
`;

test.describe("the record form", () => {
  test("an action with unsaved input saves it first, then runs", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Form jobs",
      fields: [
        { name: "title", required: true },
        { name: "note" },
        { name: "state", kind: "select", options: ["new", "done"] },
      ],
      actions: [
        {
          name: "finish",
          field: "state",
          from: "new",
          to: "done",
          assert: [{ expr: "is_set(note)", message: "Write a note first" }],
        },
      ],
    });
    const job = await api.create("root", model, { title: uid("Job") });
    await signIn("root", `/${model}/${model}?view=list&id=${job.id}`);
    const panel = page.locator("aside.record");
    await panel.locator(`#f-${model}-note`).fill("Checked the pump");
    // Finish is the next step: the one filled button.
    const finish = panel.locator(".actionrow").getByRole("button", { name: "Finish" });
    await expect(finish).toHaveClass(/primary/);
    await finish.click();
    await expect(page.locator(".toast").filter({ hasText: "is now done" })).toBeVisible();
    await expect(page.locator(".toast").filter({ hasText: "Write a note first" })).toHaveCount(0);
    const stored = await api.run("root", `${model}.get`, { id: job.id });
    expect(stored.note).toBe("Checked the pump");
    expect(stored.state).toBe("done");
    // The governed status is a stepper that only shows the way.
    await expect(panel.locator("ol.statusbar button")).toHaveCount(0);
    await expect(panel.locator("ol.statusbar li[aria-current=step]")).toHaveText("Done");
  });

  test("a refused action reads without the engine's words", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule({
      title: "Form checks",
      fields: [
        { name: "title", required: true },
        { name: "note" },
        { name: "state", kind: "select", options: ["new", "done"] },
      ],
      actions: [
        {
          name: "finish",
          field: "state",
          from: "new",
          to: "done",
          assert: [{ expr: "is_set(note)", message: "Write a note first" }],
        },
      ],
    });
    const job = await api.create("root", model, { title: uid("Job") });
    await signIn("root", `/${model}/${model}?view=list&id=${job.id}`);
    await page.locator("aside.record .actionrow").getByRole("button", { name: "Finish" }).click();
    const toast = page.locator(".toast").filter({ hasText: "Write a note first" });
    await expect(toast).toBeVisible();
    await expect(toast).not.toContainText("assertion");
  });

  test("the header names a record by what it is about, never by its id", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const party = await api.create("root", "party", { name: uid("Baumann Bau") });
    const model = await scratchModule({
      title: "Form visits",
      fields: [
        { name: "party_id", kind: "ref", to: "party", required: true },
        { name: "visit_on", kind: "date", required: true },
        { name: "price", kind: "money" },
      ],
    });
    const visit = await api.create("root", model, {
      party_id: party.id,
      visit_on: "2026-03-14",
      price: 12000,
    });
    await signIn("root", `/${model}/${model}?view=list&id=${visit.id}`);
    const title = page.locator("aside.record h2");
    await expect(title).toContainText(party.name);
    await expect(title).not.toContainText(visit.id);
    await expect(title).not.toContainText(visit.id.slice(0, 8));
  });

  test("a record with lines opens as a page, and goes back beside the list", async ({
    page,
    signIn,
    api,
    uid,
    scratchPackage,
  }) => {
    const id = await scratchPackage(withLines);
    const name = uid("Order");
    await api.create("root", `${id}_order`, { name });
    await signIn("root", `/${id}/${id}_order?view=list`);
    await page
      .getByRole("link", { name })
      .or(page.getByText(name, { exact: true }))
      .first()
      .click();
    const panel = page.locator("aside.record");
    await expect(panel).toHaveClass(/full/);
    await expect(page).toHaveURL(/full=1/);
    await expect(page.locator("section.main")).toBeHidden();
    await expect(panel.locator("h2")).toHaveText(name);
    // The lines are drawn in the form, so they get no related-records button of their own.
    await expect(panel.locator(".smartbtn", { hasText: "line" })).toHaveCount(0);
    await panel.getByRole("button", { name: "Show beside the list" }).click();
    await expect(panel).not.toHaveClass(/full/);
    await expect(page.locator("section.main")).toBeVisible();
  });
});

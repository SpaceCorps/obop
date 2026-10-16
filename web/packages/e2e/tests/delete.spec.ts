import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

const grid = (page: Page, name: string) => page.getByRole("grid", { name: `${name} list` });
const rowOf = (page: Page, name: string, text: string) =>
  grid(page, name).getByRole("row", { name: new RegExp(text) });
const panelOf = (page: Page, name: string) =>
  page.getByRole("complementary", { name: `${name} record` });

test.describe("delete", () => {
  test("a record is deleted from its panel after an in-app confirmation", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const title = uid("Doomed lead");
    const keep = uid("Kept lead");
    const lead = await api.create("root", "lead", { title });
    await api.create("root", "lead", { title: keep });
    await signIn("root", `/crm/lead?view=list&id=${lead.id}`);
    const panel = panelOf(page, "Lead");
    await panel.getByRole("button", { name: "Delete" }).click();

    // The dialog names the record; Cancel (the default focus) keeps it.
    const dialog = page.getByRole("alertdialog", { name: "Delete lead?" });
    await expect(dialog).toContainText(`Delete lead “${title}”? This cannot be undone.`);
    await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toHaveCount(0);
    expect((await api.list("root", "lead")).some((l) => l.id === lead.id)).toBe(true);

    await panel.getByRole("button", { name: "Delete" }).click();
    await dialog.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "Lead deleted" })).toBeVisible();
    await expect(panel).toHaveCount(0);
    await expect(rowOf(page, "Lead", title)).toHaveCount(0);
    await expect(rowOf(page, "Lead", keep)).toBeVisible();
    expect((await api.list("root", "lead")).some((l) => l.id === lead.id)).toBe(false);
  });

  test("a record that others still point at is refused, with links to the records that block it", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const name = uid("Referenced party");
    const party = await api.create("root", "party", { name });
    const order = await api.create("root", "sale_order", { party_id: party.id });
    await signIn("root", `/base/party?view=list&id=${party.id}`);
    await panelOf(page, "Party").getByRole("button", { name: "Delete" }).click();
    const dialog = page.getByRole("alertdialog", { name: "Delete party?" });
    await dialog.getByRole("button", { name: "Delete", exact: true }).click();

    // The server's refusal is explained in the dialog, which names the model and links to the record.
    const failed = page.getByRole("alertdialog", { name: "Could not delete party" });
    await expect(failed.getByRole("alert")).toContainText("still used by other records");
    await expect(failed).toContainText("Sale order (1)");
    const link = failed.getByRole("link");
    await expect(link).toHaveAttribute("href", new RegExp(`#/sales/sale_order\\?.*id=${order.id}`));
    expect((await api.list("root", "party")).some((p) => p.id === party.id)).toBe(true);

    // Following the link closes the dialog and opens the blocking record.
    await link.click();
    await expect(failed).toHaveCount(0);
    await expect(page).toHaveURL(new RegExp(`#/sales/sale_order.*id=${order.id}`));
    await expect(panelOf(page, "Sale order")).toBeVisible();

    // Once nothing refers to it any more, the delete goes through.
    await api.run("root", "sale_order.delete", { id: order.id });
    await page.goto(`/#/base/party?view=list&id=${party.id}`);
    await panelOf(page, "Party").getByRole("button", { name: "Delete" }).click();
    await dialog.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "Party deleted" })).toBeVisible();
    expect((await api.list("root", "party")).some((p) => p.id === party.id)).toBe(false);
  });

  test("bulk delete asks once, deletes the selection and reports how many", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    // A fixed id keeps the plural predictable ("bulkdel" to "bulkdels").
    const model = await scratchModule({
      id: "bulkdel",
      title: "Bulk",
      fields: [{ name: "title", required: true }],
    });
    const names = [uid("Bulk A"), uid("Bulk B"), uid("Bulk C")];
    for (const title of names) await api.create("root", model, { title });
    await signIn("root", `/${model}/${model}?view=list`);
    const list = grid(page, "Bulkdel");
    for (const n of names.slice(0, 2))
      await list
        .getByRole("row", { name: new RegExp(n) })
        .getByRole("checkbox")
        .check();

    const bar = page.getByRole("toolbar", { name: "Bulk actions" });
    await expect(bar).toContainText("2 selected");
    await bar.getByRole("button", { name: "Delete" }).click();
    const dialog = page.getByRole("alertdialog", { name: `Delete 2 ${model}s?` });
    await expect(dialog).toContainText("This cannot be undone");
    await dialog.getByRole("button", { name: `Delete 2 ${model}s` }).click();

    await expect(page.locator(".toast").filter({ hasText: `2 ${model}s deleted` })).toBeVisible();
    await expect(page.locator(".controlpanel .count")).toHaveText("1 record");
    expect((await api.list("root", model)).map((r) => r.title)).toEqual([names[2]]);
  });

  test("bulk delete with a record still in use shows a result for each record", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const stem = uid("Mixed");
    const used = await api.create("root", "party", { name: `${stem} used` });
    const free = await api.create("root", "party", { name: `${stem} free` });
    await api.create("root", "sale_order", { party_id: used.id });
    await signIn("root", "/base/party?view=list");
    const search = page.getByRole("combobox", { name: /^Search/ });
    await search.fill(stem);
    await search.press("Enter");
    await expect(page.locator(".controlpanel .count")).toHaveText("2 records");
    await grid(page, "Party").getByRole("checkbox", { name: "Select all records" }).check();
    await page
      .getByRole("toolbar", { name: "Bulk actions" })
      .getByRole("button", { name: "Delete" })
      .click();
    await page
      .getByRole("alertdialog", { name: "Delete 2 parties?" })
      .getByRole("button", { name: "Delete 2 parties" })
      .click();

    const done = page.getByRole("alertdialog", { name: "Delete finished" });
    await expect(done.getByRole("status")).toHaveText("1 party deleted, 1 still in use.");
    await expect(done.locator("li", { hasText: `${stem} free` })).toContainText("deleted");
    await expect(done.locator("li", { hasText: `${stem} used` })).toContainText("still in use");
    await expect(done.getByRole("link")).toHaveCount(1);
    const left = (await api.list("root", "party")).map((p) => p.id);
    expect(left).toContain(used.id);
    expect(left).not.toContain(free.id);
    await done.locator("footer").getByRole("button", { name: "Close" }).click();
    await expect(done).toHaveCount(0);
  });

  test("an agent can only propose a delete: the toast names the plan and the record stays", async ({
    page,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      id: "wfagent",
      title: "Agent delete",
      fields: [{ name: "title", required: true }],
    });
    const rec = await api.create("root", model, { title: uid("Proposed") });
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: ["wfagent.user", "wfagent.manager"], label: "delete agent" },
      await api.token("ivy"),
    );
    expect(issued.status).toBe(200);
    await page.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [issued.json.token, "agent"],
    );
    await page.goto(`/#/${model}/${model}?view=list&id=${rec.id}`);
    await page
      .getByRole("complementary", { name: /record$/ })
      .getByRole("button", { name: "Delete" })
      .click();
    await page
      .getByRole("alertdialog", { name: /^Delete/ })
      .getByRole("button", { name: "Delete", exact: true })
      .click();
    const toast = page.locator(".toast").filter({ hasText: "Approval needed" });
    await expect(toast).toContainText("deleting");
    await expect(toast).toContainText(/plan [0-9a-f]{8}/);
    await expect(toast).toContainText("Nothing is deleted until a human approves it");
    expect((await api.list("root", model)).some((r) => r.id === rec.id)).toBe(true);
  });
});

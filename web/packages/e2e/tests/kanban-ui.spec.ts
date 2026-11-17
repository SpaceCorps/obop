import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// The board of wave 1 of the UI plan (lane L3): cards are not dragged between the values of a ref
// (B9), the quick form asks for what a record needs and says what is missing in a sentence (B10),
// and a column whose amounts add up below zero shows its sum (B8).

const board = (page: Page, by = "State") =>
  page.getByRole("group", { name: new RegExp(`board by ${by}`) });
const column = (page: Page, name: string, by = "State") =>
  board(page, by).getByRole("region", { name: new RegExp(`^${name}, \\d+ cards?$`, "i") });
const card = (col: Locator, title: string) => col.locator(".kcard", { hasText: title });

test.describe("kanban board (L3)", () => {
  test("a board grouped by a ref cannot be dragged: no card moves to another record's column", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const owners = await scratchModule({
      title: "Owners",
      fields: [{ name: "title", required: true }],
    });
    const model = await scratchModule({
      title: "Owned items",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "done"] },
        { name: "owner_id", kind: "ref", to: owners },
      ],
    });
    const [ann, bob] = [uid("Ann"), uid("Bob")];
    const a = await api.create("root", owners, { title: ann });
    const b = await api.create("root", owners, { title: bob });
    const item = uid("Hammer");
    const rec = await api.create("root", model, { title: item, state: "new", owner_id: a.id });
    await api.create("root", model, { title: uid("Saw"), state: "new", owner_id: b.id });
    await signIn("root", `/${model}/${model}?view=kanban`);

    // Grouped by its status, a card may be dragged and has a menu to move it.
    const byState = card(column(page, "New"), item);
    await expect(byState).toHaveAttribute("draggable", "true");
    await expect(byState.getByRole("button", { name: /^Move .* to$/ })).toHaveCount(1);

    // The same board grouped by the owner (the search in the address, as the Group by menu writes it).
    const byOwner = encodeURIComponent(JSON.stringify({ g: ["owner_id"] }));
    await page.evaluate((h) => (location.hash = h), `/${model}/${model}?view=kanban&st=${byOwner}`);
    const annCol = column(page, ann, "Owner");
    const bobCol = column(page, bob, "Owner");
    await expect(card(annCol, item)).toHaveCount(1);
    // Grouped by a ref: no drag, no move menu, and the columns say nothing about dropping.
    await expect(card(annCol, item)).toHaveAttribute("draggable", "false");
    await expect(board(page, "Owner").getByRole("button", { name: /^Move .* to$/ })).toHaveCount(0);
    await expect(board(page, "Owner")).not.toContainText("Drop a card");

    // A drop sent anyway (the events a browser would fire) changes nothing.
    const data = await page.evaluateHandle((id) => {
      const dt = new DataTransfer();
      dt.setData("text/plain", id);
      return dt;
    }, rec.id);
    await card(annCol, item).dispatchEvent("dragstart", { dataTransfer: data });
    await bobCol.dispatchEvent("dragover", { dataTransfer: data });
    await bobCol.dispatchEvent("drop", { dataTransfer: data });
    await expect(card(annCol, item)).toHaveCount(1);
    await expect(card(bobCol, item)).toHaveCount(0);
    expect((await api.run("root", `${model}.get`, { id: rec.id })).owner_id).toBe(a.id);
  });

  test("the quick form asks for a required ref and says what is missing in a sentence", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const customers = await scratchModule({
      title: "Customers",
      fields: [{ name: "title", required: true }],
    });
    const model = await scratchModule({
      title: "Jobs",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["new", "done"] },
        { name: "customer_id", kind: "ref", to: customers, required: true },
      ],
    });
    const customer = uid("Baumann");
    const c = await api.create("root", customers, { title: customer });
    // An empty model shows its empty state instead of the board: it starts with one card.
    await api.create("root", model, { title: uid("Seed"), state: "new", customer_id: c.id });
    await signIn("root", `/${model}/${model}?view=kanban`);

    const title = uid("Quick job");
    await page.getByRole("button", { name: "Add a card to New" }).click();
    const form = page.locator("form.quick");
    await expect(form.getByLabel("Customer")).toBeVisible();
    await form.getByLabel("New title").fill(title);
    await form.getByRole("button", { name: "Add", exact: true }).click();

    // The field's own name, in a sentence: never the model's technical name or the server's words.
    const error = form.locator(".error");
    await expect(error).toHaveText("Customer is required");
    await expect(form).not.toContainText(`${model}.customer_id`);
    expect((await api.list("root", model)).some((r) => r.title === title)).toBe(false);

    // With the customer picked, the card is made, lands on top of its column and lights up.
    await form.getByLabel("Customer").click();
    await page.getByRole("option", { name: customer }).click();
    await form.getByRole("button", { name: "Add", exact: true }).click();
    const made = card(column(page, "New"), title);
    await expect(made).toHaveCount(1);
    await expect(column(page, "New").locator(".kcard").first()).toContainText(title);
    const rec = (await api.list("root", model)).find((r) => r.title === title);
    expect(rec?.state).toBe("new");
    expect(rec?.customer_id).toBeTruthy();
    // The form stays for the next card, emptied.
    await expect(form.getByLabel("New title")).toHaveValue("");
  });

  test("a column whose amounts add up below zero shows its sum, in the instance's currency", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Bank lines",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["unmatched", "matched"] },
        { name: "amount_minor", kind: "money" },
      ],
    });
    await api.create("root", model, {
      title: uid("Rent"),
      state: "matched",
      amount_minor: -150000,
    });
    await api.create("root", model, { title: uid("Fees"), state: "matched", amount_minor: -2500 });
    await api.create("root", model, {
      title: uid("Payment"),
      state: "unmatched",
      amount_minor: 4000,
    });
    await signIn("root", `/${model}/${model}?view=kanban`);

    await expect(column(page, "Matched").locator(".sum")).toHaveText("−€1,525.00");
    await expect(column(page, "Unmatched").locator(".sum")).toHaveText("€40.00");
    // The amount is on the card too, and the stage's badge keeps its whole word beside the count.
    await expect(column(page, "Matched").locator(".kcard .kmoney").first()).toContainText("−€");
    await expect(column(page, "Matched").locator(".kname")).toHaveText("Matched");
  });
});

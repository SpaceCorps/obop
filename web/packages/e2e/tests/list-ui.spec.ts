import { expect, test } from "../support/fixtures.ts";

// The list view (lane L2 of the UI plan): a row opens its record wherever it is clicked, a ref in a
// cell is text with an arrow that follows it, editing in place needs Edit in list, a double-click or
// F2, the footer adds up only what adds up (per currency), and a phone shows one card per record.

test.describe("the list view", () => {
  test("a click anywhere in a row opens the record, a ref's arrow follows the ref", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
  }) => {
    const model = await scratchModule({
      title: "Row things",
      fields: [
        { name: "title", required: true },
        { name: "customer_id", kind: "ref", to: "party" },
      ],
    });
    const name = uid("Row customer");
    const party = await api.create("root", "party", { name });
    const title = uid("Row");
    const rec = await api.create("root", model, { title, customer_id: party.id });
    await signIn("root", `/${model}/${model}?view=list`);
    const row = page.getByRole("grid").getByRole("row", { name: new RegExp(title) });

    // The ref's words open the row's own record, not the customer.
    await row.locator(".reftext-label").click();
    await expect(page).toHaveURL(new RegExp(`/${model}/${model}\\?.*id=${rec.id}`));
    await page.keyboard.press("Escape");
    await expect(page).not.toHaveURL(/id=/);

    // The first column is the record's title as text, and it opens the record too.
    await row.getByRole("gridcell", { name: title }).click();
    await expect(page).toHaveURL(new RegExp(`id=${rec.id}`));
    await page.keyboard.press("Escape");

    // The arrow, shown on hover, follows the ref to the customer.
    await row.hover();
    await row.locator(".refgo").click();
    await expect(page).toHaveURL(new RegExp(`/party\\?.*id=${party.id}`));
  });

  test("editing in place needs Edit in list, a double-click or F2", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
  }) => {
    const model = await scratchModule({
      title: "Edit things",
      fields: [{ name: "title", required: true }, { name: "note" }],
    });
    const title = uid("Editable");
    await api.create("root", model, { title, note: "before" });
    await signIn("root", `/${model}/${model}?view=list`);
    const note = page
      .getByRole("grid")
      .getByRole("row", { name: new RegExp(title) })
      .locator('td[data-f="note"]');

    // A double-click edits in place and leaves the record closed.
    await note.dblclick();
    const input = page.locator("td.editing input");
    await expect(input).toBeFocused();
    await expect(page).not.toHaveURL(/id=/);
    await input.fill("after");
    await page.keyboard.press("Enter");
    await expect(note).toHaveText("after");

    // F2 on the focused cell edits too; Escape keeps the value.
    await note.focus();
    await page.keyboard.press("F2");
    await page.locator("td.editing input").fill("never saved");
    await page.keyboard.press("Escape");
    await expect(note).toHaveText("after");

    // With Edit in list a single click edits at once.
    await page.getByRole("button", { name: "Edit in list" }).click();
    await note.click();
    await expect(page.locator("td.editing input")).toBeFocused();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Edit in list" }).click();

    const stored = (await api.list("root", model)).find((r) => r.title === title);
    expect(stored?.note).toBe("after");
  });

  test("the footer adds up only what adds up, each currency apart", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Sum things",
      fields: [
        { name: "title", required: true },
        { name: "amount_minor", kind: "money" },
        { name: "unit_price_minor", kind: "money" },
        { name: "quantity", kind: "number" },
        { name: "level", kind: "number" },
        { name: "currency" },
      ],
    });
    const rows: [string, number, string | null][] = [
      ["Sum A", 1000, "EUR"],
      ["Sum B", 250, null],
      ["Sum C", 3000, "SEK"],
    ];
    for (const [title, amount, currency] of rows)
      await api.create("root", model, {
        title,
        amount_minor: amount,
        unit_price_minor: 999,
        quantity: 2,
        level: 7,
        ...(currency ? { currency } : {}),
      });
    await signIn("root", `/${model}/${model}?view=list`);
    const foot = (f: string) => page.locator(`table.list tfoot td[data-f="${f}"]`);
    // The record without a currency counts in the instance's currency: one euro sum, not two (B7).
    await expect(foot("amount_minor")).toContainText("€12.50");
    await expect(foot("amount_minor")).toContainText(/SEK\s30\.00/);
    await expect(foot("amount_minor")).not.toContainText("€10.00");
    // A unit price and a level are not added up; a quantity is.
    await expect(foot("quantity")).toHaveText("6");
    await expect(foot("unit_price_minor")).toHaveText("");
    await expect(foot("level")).toHaveText("");
  });

  test("a list where nothing adds up has no footer", async ({
    page,
    signIn,
    api,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "No sum things",
      fields: [
        { name: "title", required: true },
        { name: "rate_bp", kind: "number" },
      ],
    });
    await api.create("root", model, { title: "Only", rate_bp: 1900 });
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(page.getByRole("gridcell", { name: "Only" })).toBeVisible();
    await expect(page.locator("table.list tfoot")).toHaveCount(0);
  });

  test("a phone shows one card per record: title, status and amount", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
  }) => {
    const model = await scratchModule({
      title: "Card things",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["draft", "done"], required: true },
        { name: "amount_minor", kind: "money" },
        { name: "due_on", kind: "date" },
      ],
    });
    const title = uid("Card");
    await api.create("root", model, {
      title,
      state: "draft",
      amount_minor: 1234,
      due_on: "2026-01-15",
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await signIn("root", `/${model}/${model}?view=list`);
    const card = page.locator("td.lcard").filter({ hasText: title });
    await expect(card.locator(".lc-title")).toHaveText(title);
    await expect(card.locator(".badge")).toHaveText("Draft");
    await expect(card.locator(".lc-money")).toHaveText("€12.34");
    // A past deadline of a record that is not settled is marked.
    await expect(card.locator(".due.past")).toBeVisible();
    // No grid of labels and values: a card is about 96 px tall.
    const box = (await card.boundingBox())!;
    expect(box.height).toBeLessThan(110);
    // No sideways scroll, and a tap opens the record.
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      390,
    );
    await card.click();
    await expect(page.locator("aside.record")).toBeVisible();
  });
});

import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// The lines of a document inside its form (LinesSection.tsx, lane L7 of the UI plan): a sale
// order gets its lines without leaving the order. Each line is the child's own create, update or
// delete. Where the form is narrow (the side panel, a phone) the lines are cards with an edit
// sheet; where it is wide, a table edited in place. The keys are the same in both: after "Add a
// line" the product field has the focus, Enter picks, Enter saves.

const panelOf = (page: Page) =>
  page.getByRole("complementary", { name: "Quotation / Order record" });
const linesOf = (panel: Locator) => panel.getByRole("region", { name: "Order lines" });

/** Add one line by its product, with the keyboard only. */
async function addLine(page: Page, panel: Locator, product: string) {
  await linesOf(panel).getByRole("button", { name: "Add a line" }).click();
  const field = page.getByRole("combobox", { name: /^Product/ }).and(page.locator(":focus"));
  await expect(field).toBeVisible();
  await page.keyboard.type(product);
  await expect(page.getByRole("option", { name: product })).toBeVisible();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
}

test.describe("lines inside the form", () => {
  test("a new sale order gets two lines in its form, and its total follows them", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const customer = await api.create("root", "party", { name: uid("Lines customer") });
    const drill = uid("Lines drill");
    const charger = uid("Lines charger");
    await api.create("root", "product", { name: drill, list_price_minor: 24900 });
    await api.create("root", "product", { name: charger, list_price_minor: 4900 });

    await signIn("root", "/sales/sale_order?view=list&id=new");
    const panel = panelOf(page);
    await panel.getByLabel("Customer").click();
    await page.keyboard.type(customer.name);
    await page.getByRole("option", { name: customer.name }).click();
    // Before the order exists, the section says the first line saves it.
    await expect(linesOf(panel)).toContainText("the record is saved with it");

    // The first "Add a line" creates the order, then opens its first line.
    await addLine(page, panel, drill);
    await expect(
      page.locator(".toast").filter({ hasText: "Quotation / Order created" }),
    ).toBeVisible();
    await expect(panel.locator(".rec-figures")).toContainText(/249\.00/);
    await addLine(page, panel, charger);
    await expect(panel.locator(".rec-figures")).toContainText(/298\.00/);

    const lines = linesOf(panel);
    await expect(lines.locator(".lines-count")).toHaveText("2");
    await expect(lines).toContainText(drill);
    await expect(lines).toContainText(charger);
    await expect(lines).toContainText("€298.00");

    // What the form wrote is what an agent would have written: two lines through the commands,
    // each with the product's price and a quantity of 1, and the order's computed total.
    const order = (await api.list("root", "sale_order")).find((o) => o.party_id === customer.id);
    expect(order.total_minor).toBe(29800);
    const rows = (await api.list("root", "sale_order_line")).filter((l) => l.order_id === order.id);
    expect(rows.map((l) => [l.description, l.quantity, l.unit_price_minor]).sort()).toEqual(
      [
        [charger, 1, 4900],
        [drill, 1, 24900],
      ].sort(),
    );
  });

  test("a line is changed and deleted in place, and the total follows", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const customer = await api.create("root", "party", { name: uid("Lines edit customer") });
    const order = await api.create("root", "sale_order", { party_id: customer.id });
    const work = await api.create("root", "sale_order_line", {
      order_id: order.id,
      description: uid("Workshop"),
      quantity: 2,
      unit_price_minor: 50000,
    });
    await api.create("root", "sale_order_line", {
      order_id: order.id,
      description: uid("Travel"),
      quantity: 1,
      unit_price_minor: 2500,
    });

    await signIn("root", `/sales/sale_order?view=list&id=${order.id}`);
    const panel = panelOf(page);
    await expect(panel.locator(".rec-figures")).toContainText(/1,?025\.00/);
    const lines = linesOf(panel);
    const cards = (await panel.locator(".lines.is-cards").count()) > 0;

    // Three of the first line.
    if (cards) {
      await lines.getByRole("button", { name: "Edit line 1" }).click();
      const sheet = page.getByRole("dialog", { name: work.description });
      await sheet.getByLabel("Quantity").fill("3");
      await sheet.getByRole("button", { name: "Save" }).click();
      await expect(sheet).toBeHidden();
    } else {
      await lines.getByLabel("Quantity, line 1").fill("3");
      await page.keyboard.press("Enter");
    }
    await expect(panel.locator(".rec-figures")).toContainText(/1,?525\.00/);
    expect((await api.run("root", "sale_order_line.get", { id: work.id })).quantity).toBe(3);

    // Then the second line goes.
    if (cards) {
      await lines.getByRole("button", { name: "Edit line 2" }).click();
      await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    } else await lines.getByRole("button", { name: "Delete line 2" }).click();
    await expect(panel.locator(".rec-figures")).toContainText(/1,?500\.00/);
    await expect(lines.locator(".lines-count")).toHaveText("1");
  });
});

import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// Field inputs and pickers (UI plan 2.7, lane L6): the ref picker finds a record by any of its
// search fields and fills what follows it, makes a record from what was typed, and a duration is
// typed the way people say it and kept in minutes.

const panelOf = (page: Page, name: string) =>
  page.getByRole("complementary", { name: `${name} record` });

test.describe("field inputs", () => {
  test("the picker finds a record by name, with one option highlighted", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Picker Kunde");
    await api.create("root", "party", { name });
    await api.create("root", "party", { name: uid("Someone else") });

    await signIn("root", "/sales/sale_order?view=list&id=new");
    const order = panelOf(page, "Sale order");
    const party = order.getByRole("combobox", { name: "Party" });
    await party.click();
    await party.fill(name.split(" ").slice(1).join(" "));
    const listbox = page.getByRole("listbox", { name: "Party options" });
    // One highlighted option: the record that matches.
    await expect(listbox.getByRole("option", { selected: true })).toHaveCount(1);
    await expect(listbox.getByRole("option", { name: new RegExp(name) })).toBeVisible();
    await listbox.getByRole("option", { name: new RegExp(name) }).click();
    await expect(party).toHaveValue(name);
  });

  test("a product is found by its SKU and picking it fills the line", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const sku = uid("PK").replace(/\s+/g, "-");
    const product = uid("Picker drill");
    await api.create("root", "product", { name: product, sku, list_price_minor: 12900 });
    // The SKU is the grey second line of the option; picking fills the description and the price.
    await signIn("root", "/sales/sale_order_line?view=list&id=new");
    const line = panelOf(page, "Sale order line");
    const picker = line.getByRole("combobox", { name: "Product" });
    await picker.click();
    await picker.fill(sku);
    const option = page
      .getByRole("listbox", { name: "Product options" })
      .getByRole("option", { name: new RegExp(product) });
    await expect(option).toContainText(sku);
    await picker.press("Enter");
    await expect(picker).toHaveValue(product);
    await expect(line.getByLabel("Description")).toHaveValue(product);
    await expect(line.getByLabel("Unit price")).toHaveValue("129.00");
  });

  test("the picker makes a record from what was typed", async ({ page, signIn, api, uid }) => {
    const name = uid("Quick party");
    await signIn("root", "/sales/sale_order?view=list&id=new");
    const order = panelOf(page, "Sale order");
    const party = order.getByRole("combobox", { name: "Party" });
    await party.click();
    await party.fill(name);
    await page.getByRole("option", { name: `Create “${name}”` }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    await expect(party).toHaveValue(name);
    const made = (await api.list("root", "party")).find((r: { name: string }) => r.name === name);
    expect(made).toBeTruthy();

    await order.getByRole("button", { name: "Create", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "Sale order created" })).toBeVisible();
    const orders = await api.list("root", "sale_order");
    expect(orders.some((o: { party_id?: string }) => o.party_id === made.id)).toBe(true);
  });

  test("a duration typed as 1:30 is stored as 90 minutes and reads 1 h 30", async ({
    page,
    signIn,
    api,
    scratchModule,
    uid,
  }) => {
    const model: string = await scratchModule({
      title: "Work log",
      fields: [
        { name: "title", required: true },
        { name: "planned_minutes", kind: "number" },
      ],
    });
    const title = uid("Visit");
    await signIn("root", `/${model}/${model}?view=list&id=new`);
    const panel = page.getByRole("complementary").first();
    await panel.getByLabel("Title").fill(title);
    const duration = panel.getByLabel("Planned time");
    await duration.fill("1:30");
    await expect(panel.getByRole("status").filter({ hasText: "1 h 30" })).toHaveCount(0);
    await duration.fill("1.5h");
    await expect(panel.getByText("Read as 1 h 30")).toBeVisible();
    await duration.fill("1:30");
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    const saved = (await api.list("root", model)).find((r: { title: string }) => r.title === title);
    expect(saved.planned_minutes).toBe(90);

    await page.getByRole("button", { name: "List", exact: true }).click();
    await expect(page.getByRole("row", { name: new RegExp(title) })).toContainText("1 h 30");
  });
});

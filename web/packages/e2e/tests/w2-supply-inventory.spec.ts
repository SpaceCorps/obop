import { expect, test, type Api } from "../support/fixtures.ts";

// Inventory (wave 2, supply): the home has the warehouse jobs as buttons. "Receive goods" puts
// goods into stock in one dialog (a receipt move, made and validated), "Scrap" takes them out of
// stock (a scrap move with no destination, from where most of them are), "Move stock" says what is
// on hand where the goods leave and asks again before the stock there goes below zero, and "Count
// stock" on a stock item books the difference as a stock count.
// Every step is the commands an agent runs: inventory_move.create and inventory_move.validate.

async function withInventory(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "inventory" }, root);
  try {
    await run();
  } finally {
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
}

async function stockOf(api: Api, product: string, location: string): Promise<number> {
  const rows = await api.list("root", "inventory_item");
  return rows
    .filter((r) => r.product_id === product && r.location_id === location)
    .reduce((n, r) => n + Number(r.quantity ?? 0), 0);
}

test("receive goods, scrap some and count the rest from the Inventory home", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withInventory(api, async () => {
    const wh = await api.create("root", "inventory_warehouse", {
      name: uid("Inv WH"),
      code: uid("W").slice(0, 12),
    });
    // Names that sort first, so the dialog's preset locations are these two.
    const dock = await api.create("root", "inventory_location", {
      name: uid("!A dock"),
      warehouse_id: wh.id,
      kind: "receiving",
    });
    const scrap = await api.create("root", "inventory_location", {
      name: uid("!A scrap"),
      warehouse_id: wh.id,
      kind: "scrap",
    });
    const shelf = await api.create("root", "inventory_location", {
      name: uid("Inv shelf"),
      warehouse_id: wh.id,
      kind: "internal",
    });
    const product = await api.create("root", "product", { name: uid("Inv drill") });

    await signIn("root", "/inventory/overview");
    await page.getByRole("button", { name: "Receive goods" }).click();
    const dialog = page.getByRole("dialog", { name: "Receive goods" });
    await dialog.getByRole("combobox", { name: "Product" }).click();
    await page.keyboard.type(product.name);
    await page.getByRole("option", { name: product.name }).click();
    await dialog.getByLabel("Quantity").fill("12");
    await expect(dialog.getByRole("combobox", { name: "To" })).toHaveValue(dock.name);
    await dialog.getByRole("button", { name: "Receive", exact: true }).click();
    await expect(page.locator(".toast").filter({ hasText: "stock updated" })).toBeVisible();
    expect(await stockOf(api, product.id, dock.id)).toBe(12);

    await page.getByRole("button", { name: "Scrap" }).click();
    const sd = page.getByRole("dialog", { name: "Scrap" });
    await sd.getByRole("combobox", { name: "Product" }).click();
    await page.keyboard.type(product.name);
    await page.getByRole("option", { name: product.name }).click();
    await sd.getByLabel("Quantity").fill("2");
    // From starts where the goods are, and says how many are there; scrap has no destination.
    await expect(sd.getByRole("combobox", { name: "From" })).toHaveValue(dock.name);
    await expect(sd).toContainText("On hand here: 12");
    await expect(sd.getByRole("combobox", { name: "To" })).toHaveCount(0);
    await sd.getByRole("button", { name: "Scrap", exact: true }).click();
    await expect(sd).toBeHidden();
    expect(await stockOf(api, product.id, dock.id)).toBe(10);
    expect(await stockOf(api, product.id, scrap.id)).toBe(0);

    // Moving more than is there: warned, and a second press goes ahead.
    await page.getByRole("button", { name: "Move stock" }).click();
    const md = page.getByRole("dialog", { name: "Move stock" });
    await md.getByRole("combobox", { name: "Product" }).click();
    await page.keyboard.type(product.name);
    await page.getByRole("option", { name: product.name }).click();
    await md.getByLabel("Quantity").fill("15");
    await expect(md.getByRole("combobox", { name: "From" })).toHaveValue(dock.name);
    await md.getByRole("combobox", { name: "To" }).click();
    await page.keyboard.type(shelf.name);
    await page.getByRole("option", { name: shelf.name }).first().click();
    await expect(md.getByRole("alert")).toContainText("Only 10 on hand");
    await md.getByRole("button", { name: "Move", exact: true }).click();
    await expect(md).toBeVisible();
    await md.getByLabel("Quantity").fill("4");
    await expect(md.getByRole("alert")).toHaveCount(0);
    await md.getByRole("button", { name: "Move", exact: true }).click();
    await expect(md).toBeHidden();
    expect(await stockOf(api, product.id, dock.id)).toBe(6);
    expect(await stockOf(api, product.id, shelf.id)).toBe(4);

    // The stock item of the dock: counted 3, so 3 leave as a stock count.
    const item = (await api.list("root", "inventory_item")).find(
      (r) => r.product_id === product.id && r.location_id === dock.id,
    );
    await signIn("root", `/inventory/inventory_item?id=${item.id}`);
    await page.getByRole("button", { name: "Count stock" }).click();
    const cd = page.getByRole("dialog", { name: "Count stock" });
    await cd.getByLabel("Counted").fill("3");
    await expect(cd).toContainText("-3");
    await cd.getByRole("button", { name: "Save count" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Stock set to 3" })).toBeVisible();
    expect(await stockOf(api, product.id, dock.id)).toBe(3);
    const moves = (await api.list("root", "inventory_move")).filter(
      (m) => m.product_id === product.id,
    );
    expect(moves.map((m) => [m.kind, m.quantity, m.state]).sort()).toEqual(
      [
        ["adjustment", 3, "done"],
        ["internal", 4, "done"],
        ["receipt", 12, "done"],
        ["scrap", 2, "done"],
      ].sort(),
    );
  });
});

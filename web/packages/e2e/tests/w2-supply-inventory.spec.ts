import { expect, test, type Api } from "../support/fixtures.ts";

// Inventory (wave 2, supply): the home has the warehouse jobs as buttons. "Receive goods" puts
// goods into stock in one dialog (a receipt move, made and validated), "Scrap" takes them out to
// the scrap location, and "Count stock" on a stock item books the difference as a stock count.
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
    await sd.getByRole("combobox", { name: "From" }).click();
    await page.keyboard.type(dock.name);
    await page.getByRole("option", { name: dock.name }).click();
    await expect(sd.getByRole("combobox", { name: "To" })).toHaveValue(scrap.name);
    await sd.getByRole("button", { name: "Scrap", exact: true }).click();
    await expect(sd).toBeHidden();
    expect(await stockOf(api, product.id, dock.id)).toBe(10);
    expect(await stockOf(api, product.id, scrap.id)).toBe(2);

    // The stock item of the dock: counted 7, so 3 leave as a stock count.
    const item = (await api.list("root", "inventory_item")).find(
      (r) => r.product_id === product.id && r.location_id === dock.id,
    );
    await signIn("root", `/inventory/inventory_item?id=${item.id}`);
    await page.getByRole("button", { name: "Count stock" }).click();
    const cd = page.getByRole("dialog", { name: "Count stock" });
    await cd.getByLabel("Counted").fill("7");
    await expect(cd).toContainText("-3");
    await cd.getByRole("button", { name: "Save count" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Stock set to 7" })).toBeVisible();
    expect(await stockOf(api, product.id, dock.id)).toBe(7);
    const moves = (await api.list("root", "inventory_move")).filter(
      (m) => m.product_id === product.id,
    );
    expect(moves.map((m) => [m.kind, m.quantity, m.state]).sort()).toEqual(
      [
        ["adjustment", 3, "done"],
        ["receipt", 12, "done"],
        ["scrap", 2, "done"],
      ].sort(),
    );
  });
});

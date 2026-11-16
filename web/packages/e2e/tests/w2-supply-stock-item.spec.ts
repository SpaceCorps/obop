import { expect, test } from "../support/fixtures.ts";

// Inventory (wave 2, supply, plan bug B9): a stock item keeps its product, location and lot once it
// exists (`[model.guard] mutable_fields` alone), so stock never changes name or place without a
// move. Only the reorder level is edited; the quantity moves through stock moves. The server holds
// it for every surface, and the form draws those fields read-only.

test("a stock item keeps its product and location; only its reorder level changes", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "inventory" }, root);
  try {
    const wh = await api.create("root", "inventory_warehouse", {
      name: uid("Fix WH"),
      code: uid("F").slice(0, 12),
    });
    const shelf = await api.create("root", "inventory_location", {
      name: uid("Fix shelf"),
      warehouse_id: wh.id,
      kind: "internal",
    });
    const pliers = await api.create("root", "product", { name: uid("Fix pliers") });
    const charger = await api.create("root", "product", { name: uid("Fix charger") });
    const m = await api.create("root", "inventory_move", {
      product_id: pliers.id,
      to_location_id: shelf.id,
      quantity: 3,
      kind: "receipt",
    });
    await api.run("root", "inventory_move.validate", { id: m.id });
    const item = (await api.list("root", "inventory_item")).find(
      (r) => r.product_id === pliers.id && r.location_id === shelf.id,
    );
    expect(item?.quantity).toBe(3);

    // Over REST: relabelling the item is refused; the reorder level is not.
    const relabel = await api.post(
      "/api/inventory_item/update",
      { id: item.id, product_id: charger.id },
      root,
    );
    expect(relabel.status).toBe(409);
    expect(JSON.stringify(relabel.json)).toContain("keeps what it was created with");
    const level = await api.post(
      "/api/inventory_item/update",
      { id: item.id, reorder_level: 5 },
      root,
    );
    expect(level.status).toBe(200);

    // In the form: the guard's sentence, the product read-only, the reorder level editable.
    await signIn("root", `/inventory/inventory_item?id=${item.id}`);
    const panel = page.getByRole("complementary").first();
    await expect(panel).toContainText("A stock item keeps its product, location and lot.");
    await expect(panel.getByRole("combobox", { name: "Product" })).toHaveCount(0);
    await expect(panel.getByLabel("Reorder level")).toBeEditable();

    // At or below the reorder level now (3 <= 5): low stock.
    const after = (await api.list("root", "inventory_item")).find((r) => r.id === item.id);
    expect(after?.product_id).toBe(pliers.id);
    expect(after?.below_reorder).toBe(true);
  } finally {
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
});

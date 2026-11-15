import { expect, test } from "../support/fixtures.ts";

// Products (wave 2, supply): with Inventory installed a product shows the units on hand and the
// locations that hold them, summed on the server (inventory_item.search with aggregate), with a
// link to its stock list.

test("a product shows its units on hand and where they are", async ({ page, signIn, api, uid }) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "inventory" }, root);
  try {
    const wh = await api.create("root", "inventory_warehouse", {
      name: uid("OH WH"),
      code: uid("O").slice(0, 12),
    });
    const a = await api.create("root", "inventory_location", {
      name: uid("OH shelf A"),
      warehouse_id: wh.id,
      kind: "internal",
    });
    const b = await api.create("root", "inventory_location", {
      name: uid("OH shelf B"),
      warehouse_id: wh.id,
      kind: "internal",
    });
    const product = await api.create("root", "product", { name: uid("OH saw") });
    for (const [loc, quantity] of [
      [a.id, 5],
      [b.id, 3],
    ] as const) {
      const m = await api.create("root", "inventory_move", {
        product_id: product.id,
        to_location_id: loc,
        quantity,
      });
      await api.run("root", "inventory_move.validate", { id: m.id });
    }

    await signIn("root", `/product/product?id=${product.id}`);
    const onHand = page.getByRole("region", { name: "On hand" });
    await expect(onHand.locator(".sup-onhand-n")).toHaveText("8");
    await expect(onHand).toContainText(a.name);
    await expect(onHand).toContainText(b.name);
    await onHand.getByRole("link", { name: "Stock" }).click();
    await expect(page).toHaveURL(/inventory_item/);
    await expect(page.getByRole("row").filter({ hasText: a.name })).toBeVisible();
  } finally {
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
});

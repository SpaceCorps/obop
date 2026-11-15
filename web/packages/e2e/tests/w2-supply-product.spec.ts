import { expect, test } from "../support/fixtures.ts";

// Products (wave 2, supply): with Inventory installed a product shows where its units are, per
// location, summed on the server (inventory_item.search with aggregate), with a link to its stock;
// units at a scrap location are shown apart and not counted as stock.

test("a product shows where its units are", async ({ page, signIn, api, uid }) => {
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
    // Units at a scrap location are not stock (the seeded data and moves before 0.5.0 keep some).
    const scrap = await api.create("root", "inventory_location", {
      name: uid("OH scrap"),
      warehouse_id: wh.id,
      kind: "scrap",
    });
    const product = await api.create("root", "product", { name: uid("OH saw") });
    for (const [loc, quantity] of [
      [a.id, 5],
      [b.id, 3],
      [scrap.id, 2],
    ] as const) {
      const m = await api.create("root", "inventory_move", {
        product_id: product.id,
        to_location_id: loc,
        quantity,
      });
      await api.run("root", "inventory_move.validate", { id: m.id });
    }

    await signIn("root", `/product/product?id=${product.id}`);
    const onHand = page.getByRole("region", { name: "Stock by location" });
    await expect(onHand.getByRole("listitem").filter({ hasText: a.name })).toContainText("5");
    await expect(onHand.getByRole("listitem").filter({ hasText: b.name })).toContainText("3");
    await expect(onHand).toContainText(a.name);
    await expect(onHand).toContainText(b.name);
    await expect(onHand).toContainText("8 on hand");
    await expect(onHand.getByRole("listitem").filter({ hasText: scrap.name })).toHaveCount(0);
    await expect(onHand).toContainText("2 scrapped, not counted as stock");
    await onHand.getByRole("link", { name: "All stock" }).click();
    await expect(page).toHaveURL(/inventory_item/);
    await expect(page.getByRole("row").filter({ hasText: a.name })).toBeVisible();
  } finally {
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
});

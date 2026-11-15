import { expect, test, type Api } from "../support/fixtures.ts";

// Manufacturing (wave 2, supply): an order from start to finish. Its components come from the bill
// of materials scaled to the quantity (manufacturing_order_line.bulk_create), the panel says what
// is short at the production location, and Finish makes the production moves
// (manufacturing_order.finish) and validates them, so the components leave stock and the product
// arrives.

async function withManufacturing(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "inventory" }, root);
  await api.post("/api/marketplace_listing/install", { id: "manufacturing" }, root);
  try {
    await run();
  } finally {
    await api.post("/api/module/uninstall", { id: "manufacturing" }, root);
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
}

test("a manufacturing order uses its components and makes its product", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withManufacturing(api, async () => {
    const wh = await api.create("root", "inventory_warehouse", {
      name: uid("MO WH"),
      code: uid("M").slice(0, 12),
    });
    const floor = await api.create("root", "inventory_location", {
      name: uid("MO floor"),
      warehouse_id: wh.id,
      kind: "internal",
    });
    const kit = await api.create("root", "product", { name: uid("Tool kit") });
    const box = await api.create("root", "product", { name: uid("Box") });
    const bits = await api.create("root", "product", { name: uid("Bits") });
    // Ten boxes on the floor; no bits.
    const m = await api.create("root", "inventory_move", {
      product_id: box.id,
      to_location_id: floor.id,
      quantity: 10,
      kind: "receipt",
    });
    await api.run("root", "inventory_move.validate", { id: m.id });
    const bom = await api.create("root", "manufacturing_bom", {
      product_id: kit.id,
      name: uid("Kit recipe"),
      quantity: 2,
    });
    await api.create("root", "manufacturing_bom_line", {
      bom_id: bom.id,
      component_id: box.id,
      quantity: 2,
    });
    await api.create("root", "manufacturing_bom_line", {
      bom_id: bom.id,
      component_id: bits.id,
      quantity: 1,
    });
    const mo = await api.create("root", "manufacturing_order", {
      bom_id: bom.id,
      quantity: 4,
      location_id: floor.id,
    });
    expect(mo.product_id).toBe(kit.id);
    expect(mo.number).toMatch(/^MO-\d{5}$/);

    await signIn("root", `/manufacturing/manufacturing_order?id=${mo.id}`);
    const panel = page.getByRole("complementary", { name: "Manufacturing order record" });
    await panel.getByRole("button", { name: "Add components from the bill of materials" }).click();
    await expect(page.locator(".toast").filter({ hasText: "2 components added" })).toBeVisible();
    // Four kits are two runs: four boxes (in stock) and two bit sets (none).
    await expect(panel.locator(".sup-short")).toContainText("1 of 2 components are short");
    await expect(panel.locator(".sup-shortlist")).toContainText("needs 2, has 0");

    await panel.getByRole("button", { name: "Confirm", exact: true }).click();
    await panel.getByRole("button", { name: "Start", exact: true }).click();
    await panel.getByRole("button", { name: "Finish", exact: true }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "Finish" }).click();
    await expect(
      page.locator(".toast").filter({ hasText: "Production finished; stock updated" }),
    ).toBeVisible();
    await expect(panel.locator(".sup-receipts")).toContainText(
      "The components left stock and the product arrived.",
    );

    const items = (await api.list("root", "inventory_item")).filter(
      (r) => r.location_id === floor.id,
    );
    const at = (p: string) => items.find((r) => r.product_id === p)?.quantity;
    expect(at(kit.id)).toBe(4);
    expect(at(box.id)).toBe(6);
    expect(at(bits.id)).toBe(-2);
    const moves = (await api.list("root", "inventory_move")).filter((x) => x.origin === mo.number);
    expect(moves.every((x) => x.kind === "production" && x.state === "done")).toBe(true);
    expect(moves).toHaveLength(3);
  });
});

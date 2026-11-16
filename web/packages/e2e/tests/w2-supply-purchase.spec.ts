import { expect, test, type Api } from "../support/fixtures.ts";

// Purchase (wave 2, supply): a confirmed order is received in one dialog. The lines show what was
// ordered with a stepper for what arrived; the dialog writes a short line's received quantity,
// runs purchase_order.receive (one draft move per line, named after the order) and validates the
// moves, so the goods are in stock when it closes. The order then shows its receipts.

async function withPurchase(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "inventory" }, root);
  await api.post("/api/marketplace_listing/install", { id: "purchase" }, root);
  try {
    await run();
  } finally {
    await api.post("/api/module/uninstall", { id: "purchase" }, root);
    await api.post("/api/module/uninstall", { id: "inventory" }, root);
  }
}

test("a short delivery is received against the order and lands in stock", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPurchase(api, async () => {
    const wh = await api.create("root", "inventory_warehouse", {
      name: uid("PO WH"),
      code: uid("P").slice(0, 12),
    });
    const dock = await api.create("root", "inventory_location", {
      name: uid("PO dock"),
      warehouse_id: wh.id,
      kind: "receiving",
    });
    const gloves = await api.create("root", "product", { name: uid("Gloves"), cost_minor: 300 });
    const goggles = await api.create("root", "product", { name: uid("Goggles"), cost_minor: 900 });
    const party = await api.create("root", "party", { name: uid("PSA GmbH") });
    const vendor = await api.create("root", "purchase_vendor", { party_id: party.id });
    const po = await api.create("root", "purchase_order", {
      vendor_id: vendor.id,
      receive_location_id: dock.id,
    });
    await api.create("root", "purchase_order_line", {
      order_id: po.id,
      product_id: gloves.id,
      quantity: 10,
    });
    await api.create("root", "purchase_order_line", {
      order_id: po.id,
      product_id: goggles.id,
      quantity: 4,
    });
    await api.run("root", "purchase_order.send", { id: po.id });
    await api.run("root", "purchase_order.confirm", { id: po.id });

    await signIn("root", `/purchase/purchase_order?id=${po.id}`);
    const panel = page.getByRole("complementary", { name: "Purchase order record" });
    // The generic Receive is replaced by the dialog's button.
    await expect(panel.getByRole("button", { name: "Receive", exact: true })).toHaveCount(0);
    await panel.getByRole("button", { name: "Receive goods" }).click();
    const dialog = page.getByRole("dialog", { name: `Receive ${po.number}` });
    const glovesRow = dialog.locator(".sup-line").filter({ hasText: gloves.name });
    await expect(glovesRow.getByRole("textbox")).toHaveValue("10");
    // Three pairs short.
    for (let i = 0; i < 3; i++) await glovesRow.getByRole("button", { name: "One less" }).click();
    await expect(glovesRow).toContainText("3 short");
    await dialog.getByRole("button", { name: "Receive 11 units" }).click();
    await expect(
      page.locator(".toast").filter({ hasText: `${po.number} received: 11 units in stock` }),
    ).toBeVisible();
    await expect(panel.locator(".sup-receipts")).toContainText("11 units received into stock");
    await expect(panel.getByRole("link", { name: "Receipts (2)" })).toBeVisible();

    // What the dialog did is what an agent does: the line says 7 arrived, the moves are done
    // receipts named after the order, and the stock is there.
    const lines = (await api.list("root", "purchase_order_line")).filter(
      (l) => l.order_id === po.id,
    );
    expect(lines.find((l) => l.product_id === gloves.id).received_quantity).toBe(7);
    const moves = (await api.list("root", "inventory_move")).filter((m) => m.origin === po.number);
    expect(moves.map((m) => [m.kind, m.state, m.quantity]).sort()).toEqual(
      [
        ["receipt", "done", 4],
        ["receipt", "done", 7],
      ].sort(),
    );
    const items = (await api.list("root", "inventory_item")).filter(
      (r) => r.location_id === dock.id,
    );
    expect(items.find((r) => r.product_id === gloves.id).quantity).toBe(7);
    expect(items.find((r) => r.product_id === goggles.id).quantity).toBe(4);
    const after = (await api.list("root", "purchase_order")).find((o) => o.id === po.id);
    expect(after.state).toBe("received");
  });
});

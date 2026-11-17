import { expect, test, type Api } from "../support/fixtures.ts";

// A purchase order line (docs/ISSUES.md Q13, purchase 0.4.1): a new line takes its description from
// the product, as a sale order line does. Its received quantity stays an input of the line: Receive
// goods takes no quantities, so the line is the one place a person records a short delivery.

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

test("a new purchase line is described by its product and keeps its Received box (Q13)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  test.setTimeout(90_000);
  await withPurchase(api, async () => {
    const gloves = await api.create("root", "product", { name: uid("Gloves"), cost_minor: 300 });
    const party = await api.create("root", "party", { name: uid("PSA GmbH") });
    const vendor = await api.create("root", "purchase_vendor", { party_id: party.id });
    const po = await api.create("root", "purchase_order", { vendor_id: vendor.id });

    await signIn("root", `/purchase/purchase_order?id=${po.id}`);
    const panel = page.getByRole("complementary", { name: "Purchase order record" });
    const lines = panel.getByRole("region", { name: "Order lines" });
    await lines.getByRole("button", { name: "Add a line" }).click();
    const product = page.getByRole("combobox", { name: /^Product/ }).and(page.locator(":focus"));
    await expect(product).toBeVisible();
    await page.keyboard.type(gloves.name);
    await expect(page.getByRole("option", { name: gloves.name })).toBeVisible();
    await page.keyboard.press("Enter");
    // A short delivery is still recorded on the line, before Receive goods.
    await expect(
      page
        .getByRole("textbox", { name: /^Received/ })
        .or(page.getByRole("spinbutton", { name: /^Received/ })),
    ).toHaveCount(1);
    await page.keyboard.press("Enter");
    await expect
      .poll(async () =>
        (await api.list("root", "purchase_order_line"))
          .filter((l) => l.order_id === po.id)
          .map((l) => l.description),
      )
      .toEqual([gloves.name]);
  });
});

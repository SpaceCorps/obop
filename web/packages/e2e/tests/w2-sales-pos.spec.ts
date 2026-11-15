import { expect, test, type Api } from "../support/fixtures.ts";

// The Point of Sale register (UI plan 3.3): open a session, ring up a sale from the product grid,
// pay by card or with cash and change, see the receipt, and close the session with a cash count.
// The bundled package is installed for the test and removed afterwards; its records stay, so each
// test makes its own till and product with unique names.

async function withPos(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  const r = await api.post("/api/marketplace_listing/install", { id: "pos" }, root);
  expect(r.status, JSON.stringify(r.json)).toBe(200);
  try {
    // A till left open by an earlier test would be offered for resuming; close every open one.
    for (const s of await api.list("root", "pos_session"))
      if (s.state === "open") await api.run("root", "pos_session.close", { id: s.id });
    await run();
  } finally {
    await api.post("/api/module/uninstall", { id: "pos" }, root);
  }
}

async function tillAndProduct(api: Api, uid: (p: string) => string) {
  const till = await api.create("root", "pos_config", {
    name: uid("Till"),
    currency: "EUR",
    default_opening_cash: 15000,
  });
  // Only this till may be offered: retire the others an earlier test made.
  for (const c of await api.list("root", "pos_config"))
    if (c.id !== till.id && c.active !== false)
      await api.run("root", "pos_config.update", { id: c.id, active: false });
  const product = await api.create("root", "product", {
    name: uid("Register drill"),
    sku: uid("REG").split(" ").join("-"),
    list_price_minor: 24900,
  });
  return { till, product };
}

test.describe("point of sale register", () => {
  test("a cashier opens the till, sells two items by card and closes with a count", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    await withPos(api, async () => {
      const { till, product } = await tillAndProduct(api, uid);
      await signIn("root", "/pos");
      await expect(page.getByRole("heading", { name: "Open the register" })).toBeVisible();
      await expect(page.getByLabel("Cash in the drawer")).toHaveValue("150.00");
      await page.getByRole("button", { name: "Open register" }).click();
      await expect(page.getByRole("heading", { name: till.name })).toBeVisible();

      // Find the product by its code, as a scanner types it, then tap its tile once more.
      const search = page.getByLabel("Search products or scan a code");
      await search.fill(product.sku);
      await search.press("Enter");
      await expect(search).toHaveValue("");
      await search.fill(product.name);
      await page.getByRole("button", { name: new RegExp(`^Add ${product.name}`) }).click();
      await expect(page.getByRole("button", { name: "Pay €498.00" })).toBeVisible();

      await page.getByRole("button", { name: "Pay €498.00" }).click();
      const pay = page.getByRole("dialog", { name: "Payment" });
      await pay.getByRole("radio", { name: "Card" }).click();
      await pay.getByRole("button", { name: "Complete sale" }).click();
      const receipt = page.getByRole("dialog", { name: "Sale complete" });
      await expect(receipt).toContainText(/R-\d{4}-\d{6}/);
      await expect(receipt).toContainText("2 × " + product.name);
      await expect(receipt).toContainText("€498.00");

      const orders = (await api.list("root", "pos_order")).filter(
        (o) => o.state === "paid" && o.total === 49800,
      );
      expect(orders.length).toBeGreaterThan(0);
      const payments = (await api.list("root", "pos_payment")).filter(
        (x) => x.order_id === orders[0].id,
      );
      expect(payments.map((x) => [x.method, x.amount])).toEqual([["card", 49800]]);

      await receipt.getByRole("button", { name: "New sale" }).click();
      await page.getByRole("button", { name: "Close register" }).click();
      const close = page.getByRole("dialog", { name: "Close the register" });
      await expect(close.getByLabel("Cash counted")).toHaveValue("150.00");
      await close.getByRole("button", { name: "Close register" }).click();
      await expect(page.getByRole("heading", { name: "Open the register" })).toBeVisible();
      const session = (await api.list("root", "pos_session")).find((s) => s.config_id === till.id);
      expect(session.state).toBe("closed");
      expect(session.closing_cash).toBe(15000);
      expect(session.cash_difference).toBe(0);
    });
  });

  test("on a phone the sale is reviewed from the bar and paid in cash with change", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await withPos(api, async () => {
      const { product } = await tillAndProduct(api, uid);
      await signIn("root", "/pos");
      await page.getByRole("button", { name: "Open register" }).click();
      await page.getByLabel("Search products or scan a code").fill(product.name);
      await page.getByRole("button", { name: new RegExp(`^Add ${product.name}`) }).click();
      const bar = page.getByRole("button", { name: /1 item/ });
      await expect(bar).toBeVisible();
      await expect(page.locator("html")).toHaveJSProperty("scrollWidth", 390);
      await bar.click();
      await page.getByRole("button", { name: "Pay €249.00" }).click();
      const pay = page.getByRole("dialog", { name: "Payment" });
      await pay.getByLabel("Cash received").fill("300");
      await expect(pay).toContainText("€51.00");
      await pay.getByRole("button", { name: "Complete sale" }).click();
      const receipt = page.getByRole("dialog", { name: "Sale complete" });
      await expect(receipt.getByRole("status")).toContainText("€51.00");
      const paid = (await api.list("root", "pos_payment")).filter(
        (x) => x.method === "cash" && x.amount === 24900,
      );
      expect(paid.length).toBeGreaterThan(0);
    });
  });
});

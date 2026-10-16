import { expect, test } from "../support/fixtures.ts";

test.describe("a restricted user (sam: base, crm, sales, product)", () => {
  test("sees only the apps their groups allow, and a hidden app's address shows nothing of it", async ({
    page,
    signIn,
  }) => {
    await signIn("sam");
    await page.getByRole("button", { name: "Open the app launcher" }).click();
    const launcher = page.getByRole("dialog", { name: "Apps" });
    for (const app of ["Contacts", "Products", "CRM", "Sales"])
      await expect(launcher.getByRole("link", { name: app })).toBeVisible();
    for (const hidden of ["Invoicing", "Customize"])
      await expect(launcher.getByRole("link", { name: hidden })).toHaveCount(0);
    await page.keyboard.press("Escape");

    await page.goto("/#/invoicing/invoice");
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
    await expect(page.getByRole("region", { name: "Invoice" })).toHaveCount(0);
    await page.goto("/#/customize/modules?tab=modules");
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
    await expect(page.getByText("Marketplace")).toHaveCount(0);
  });

  test("fields their groups withhold are neither shown nor editable, while an admin sees them", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Costly");
    await api.create("root", "product", {
      name,
      sku: name.slice(0, 20),
      list_price_minor: 5000,
      cost_minor: 3000,
    });

    await signIn("sam", "/product/product?view=list");
    const grid = page.getByRole("grid", { name: "Product list" });
    const row = grid.getByRole("row", { name: new RegExp(name) });
    await expect(row).toContainText("€50.00");
    await expect(grid.getByRole("columnheader", { name: /^Cost/ })).toHaveCount(0);
    await expect(page.getByText("Withheld from you: Cost")).toBeVisible();
    await row.getByRole("gridcell").nth(1).click();
    const panel = page.getByRole("complementary", { name: "Product record" });
    await expect(panel.getByLabel("Cost")).toHaveCount(0);
    await expect(panel.getByText(/Withheld from you: Cost/)).toBeVisible();

    // The same list for the admin carries the cost.
    const admin = await api.run("root", "product.list");
    expect(admin.find((p: any) => p.name === name).cost_minor).toBe(3000);
    const sam = await api.post("/api/product/list", {}, await api.token("sam"));
    expect(sam.json.denied).toContain("product.cost_minor");
    expect(sam.json.value.find((p: any) => p.name === name)).not.toHaveProperty("cost_minor");
  });

  test("commands they may not run look exactly like commands that do not exist (404), and the catalog leaves them out", async ({
    api,
  }) => {
    const sam = await api.token("sam");
    const missing = await api.post("/api/no_such_model/list", {}, sam);
    for (const id of [
      "invoice/list",
      "invoice/post",
      "module/list",
      "module/install",
      "setting/update",
      "marketplace_listing/list",
    ]) {
      const r = await api.post(`/api/${id}`, {}, sam);
      expect(r.status, id).toBe(404);
      expect(r.json, id).toEqual(missing.json);
    }
    const catalog = await api.get("/api/commands", sam);
    const ids: string[] = catalog.json.commands.map((c: any) => c.id);
    expect(ids).toContain("party.list");
    expect(ids).toContain("lead.mark_won");
    expect(ids.filter((i) => /^(invoice|module|marketplace_listing)\./.test(i))).toEqual([]);
    // The admin does get them.
    const admin = await api.get("/api/commands", await api.token("root"));
    expect(admin.json.commands.map((c: any) => c.id)).toContain("invoice.post");
  });

  test("requests without a valid token are refused with 401", async ({ api }) => {
    expect((await api.post("/api/party/list", {})).status).toBe(401);
    expect((await api.post("/api/party/list", {}, "not-a-token")).status).toBe(401);
    expect((await api.get("/api/meta")).status).toBe(401);
    expect((await api.get("/api/commands", "not-a-token")).status).toBe(401);
  });
});

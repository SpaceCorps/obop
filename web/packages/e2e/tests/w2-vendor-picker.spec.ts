import type { Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";

// The Vendor picker of a purchase order (a vendor is named by its contact): typing a name finds
// the contact of that name whatever its case and spacing, never makes a second contact for a name
// two contacts share (the full form opens instead), and offers to restore an archived vendor of
// that contact instead of making a second vendor.

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

async function rows(api: Api, model: string, field: string, value: string) {
  const r = await api.run("root", `${model}.search`, {
    filter: [{ field, op: "eq", value }],
    limit: 10,
  });
  return r.rows as { id: string; name?: string; active?: boolean }[];
}

async function vendorPicker(page: Page, signIn: (u: "root", path: string) => Promise<void>) {
  await signIn("root", "/purchase/purchase_order?view=list&id=new");
  const panel = page.getByRole("complementary", { name: "Purchase order record" });
  return panel.getByRole("combobox", { name: "Vendor" });
}

test("a vendor typed in another case or spacing is made for the contact that exists", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPurchase(api, async () => {
    const name = uid("Baumann Bau GmbH");
    const party = await api.create("root", "party", { name });
    const picker = await vendorPicker(page, signIn);
    const typed = name.toLowerCase().replace("bau ", "bau  ");
    await picker.click();
    await picker.fill(typed);
    await page.getByRole("option", { name: `Create “${typed}”` }).click();
    await expect(picker).toHaveValue(name);
    // The toast names the vendor as it is called, not as it was typed.
    await expect(
      page.locator(".toast").getByText(`Vendor “${name}” created`, { exact: true }),
    ).toBeVisible();
    // No second contact: the vendor is the one contact of that name.
    const all = await api.run("root", "party.search", {
      filter: [{ field: "name", op: "contains", value: name.split(" ").at(-1) }],
      limit: 10,
    });
    expect(all.rows.length).toBe(1);
    expect((await rows(api, "purchase_vendor", "party_id", party.id)).length).toBe(1);
  });
});

test("a name two contacts share opens the full form instead of making a third", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPurchase(api, async () => {
    const name = uid("Doppel Kontakt GmbH");
    await api.create("root", "party", { name });
    await api.create("root", "party", { name });
    const picker = await vendorPicker(page, signIn);
    await picker.click();
    await picker.fill(name);
    await page.getByRole("option", { name: `Create “${name}”` }).click();
    await expect(page.getByRole("dialog", { name: "New vendor" })).toBeVisible();
    expect((await rows(api, "party", "name", name)).length).toBe(2);
  });
});

test("an archived vendor of the contact is restored, not made a second time", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPurchase(api, async () => {
    const name = uid("Einzel Kontakt GmbH");
    const party = await api.create("root", "party", { name });
    const vendor = await api.create("root", "purchase_vendor", { party_id: party.id });
    await api.run("root", "purchase_vendor.update", { id: vendor.id, active: false });
    const picker = await vendorPicker(page, signIn);
    await picker.click();
    await picker.fill(name);
    await page.getByRole("option", { name: `Create “${name}”` }).click();
    await page.getByRole("button", { name: "Restore", exact: true }).click();
    await expect(picker).toHaveValue(name);
    const vendors = await rows(api, "purchase_vendor", "party_id", party.id);
    expect(vendors.map((v) => [v.id, v.active])).toEqual([[vendor.id, true]]);
  });
});

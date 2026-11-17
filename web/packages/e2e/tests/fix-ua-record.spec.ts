import { expect, test, type Api } from "../support/fixtures.ts";

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

test("Create and edit keeps the typed name as the search of the contact picker (Q20)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPurchase(api, async () => {
    const name = uid("Zwilling Kontakt GmbH");
    await api.create("root", "party", { name });
    await api.create("root", "party", { name });
    await signIn("root", "/purchase/purchase_order?view=list&id=new");
    const panel = page.getByRole("complementary", { name: "Purchase order record" });
    const picker = panel.getByRole("combobox", { name: "Vendor" });
    await picker.click();
    await picker.fill(name);
    await page.getByRole("option", { name: `Create “${name}”` }).click();
    const dialog = page.getByRole("dialog", { name: "New vendor" });
    await expect(dialog).toBeVisible();
    const contact = dialog.getByRole("combobox", { name: "Contact" });
    await expect(contact).toHaveValue(name);
    await expect(page.getByRole("option", { name: new RegExp(name) })).toHaveCount(2);
  });
});

test("an address that names a field opens the record with that field in view and focused (Q19)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const party = await api.create("root", "party", { name: uid("Field link"), email: "f@x.test" });
  await page.setViewportSize({ width: 1440, height: 900 });
  await signIn("root", `/base/party?view=list&id=${party.id}&field=email`);
  const panel = page.getByRole("complementary", { name: "Contact record" });
  const email = panel.getByLabel("Email", { exact: true });
  await expect(email).toBeFocused();
  await expect(email).toBeInViewport();
  // A later link to another field of the record that is already open moves there too.
  for (const field of ["tax_id", "notes"]) {
    await page.evaluate(
      ([id, f]) => {
        location.hash = `#/base/party?view=list&id=${id}&field=${f}`;
      },
      [party.id, field],
    );
    await expect
      .poll(() =>
        page.evaluate((f) => {
          const box = document.getElementById(`f-party-${f}`);
          const a = document.activeElement;
          return !!box && !!a && (box === a || box.contains(a));
        }, field),
      )
      .toBe(true);
  }
});

test("after Create, Back does not open an empty new form again (R16)", async ({
  page,
  signIn,
  uid,
}) => {
  await signIn("root", "/crm/lead?view=list");
  await page.getByRole("button", { name: "New", exact: true }).click();
  const panel = page.getByRole("complementary", { name: "Lead record" });
  await panel.getByLabel("Title").fill(uid("Back lead"));
  await panel.getByRole("button", { name: "Create" }).click();
  await expect(page).toHaveURL(/id=(?!new)[0-9a-f-]{8,}/);
  await page.goBack();
  await expect(page).not.toHaveURL(/id=new/);
  await expect(page.getByRole("heading", { name: "New lead" })).toHaveCount(0);
});

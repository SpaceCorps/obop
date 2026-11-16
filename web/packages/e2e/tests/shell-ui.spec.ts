import { expect, test } from "../support/fixtures.ts";

// The shell (lane L1 of the UI plan): every app opens on its home, the command palette finds
// records by name through `<model>.search`, and a switch that gets focus in a side panel does not
// scroll the page into blank space (B1).

test.describe("apps open on their home", () => {
  test("a tile opens the main list of a single-list app and the overview of a larger app", async ({
    page,
    signIn,
  }) => {
    await signIn("root");
    // Sales: the order is the main model; its lines are reached through the order.
    await page
      .locator(".home")
      .getByRole("link", { name: /^Sales/ })
      .click();
    await expect(page).toHaveURL(/#\/sales\/sale_order$/);
    const sales = page.getByRole("navigation", { name: "Sales menu" });
    await expect(sales.getByRole("link", { name: "Quotations & orders" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(sales.getByRole("link", { name: /Order lines/ })).toHaveCount(0);

    // CRM: one list, with its quick searches in the sidebar.
    await page.goto("/#/");
    await page.locator(".home").getByRole("link", { name: /^CRM/ }).click();
    await expect(page).toHaveURL(/#\/crm\/lead$/);
    await expect(
      page.getByRole("navigation", { name: "CRM menu" }).getByRole("link", { name: "Open" }),
    ).toBeVisible();

    // Mileage declares its home (D95): it opens on its trips, filtered to the ones to review.
    await page.goto("/#/");
    await page
      .locator(".home")
      .getByRole("link", { name: /^Mileage/ })
      .click();
    await expect(page).toHaveURL(/#\/mileage\/mileage_trip\?st=/);

    // Connectors has several main lists and no home: it opens on its generated overview.
    await page.goto("/#/");
    await page
      .locator(".home")
      .getByRole("link", { name: /^Connectors/ })
      .click();
    await expect(page).toHaveURL(/#\/connectors\/overview$/);
    await expect(
      page
        .getByRole("navigation", { name: "Connectors menu" })
        .getByRole("link", { name: "Overview" }),
    ).toHaveAttribute("aria-current", "page");
  });

  test("an app's address without a model opens its home", async ({ page, signIn }) => {
    await signIn("root", "/sales");
    await expect(page.getByRole("region", { name: "Quotation / Order" })).toBeVisible();
    await page.goto("/#/mileage");
    await expect(page.getByRole("region", { name: "Trip" })).toBeVisible();
  });

  test("the home groups the apps by what they are for and describes each", async ({
    page,
    signIn,
  }) => {
    await signIn("root");
    const sales = page.getByRole("region", { name: "Sales", exact: true });
    await expect(sales.getByRole("link", { name: /^CRM/ })).toBeVisible();
    // The blurb is the module's summary, not the names of its models.
    await expect(sales.getByRole("link", { name: /^CRM/ })).toContainText("pipeline");
    await expect(
      page.getByRole("region", { name: "Finance" }).getByRole("link", { name: /^Invoicing/ }),
    ).toBeVisible();
  });
});

test.describe("the command palette finds records", () => {
  test("typing part of a name lists the record under its app, and Enter opens it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Palettefind");
    const party = await api.create("root", "party", { name });
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("textbox").fill(name.split(" ")[1]!);
    const hit = palette.getByRole("option", { name: new RegExp(name) });
    await expect(hit).toBeVisible();
    // Grouped under the app it belongs to.
    await expect(palette.locator("li.pal-group", { hasText: "Contacts" })).toBeVisible();
    await hit.click();
    await expect(palette).toHaveCount(0);
    await expect(page).toHaveURL(new RegExp(`#/base/party\\?id=${party.id}`));
    await expect(page.getByRole("complementary", { name: /record$/ })).toBeVisible();

    // Opened once, it is offered again before anything is typed.
    await page.keyboard.press("Escape");
    await page.keyboard.press("ControlOrMeta+k");
    await expect(palette.getByRole("option", { name: new RegExp(name) })).toBeVisible();
  });

  test("a Customize entry opens its own tab, not Theme (B6)", async ({ page, signIn }) => {
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("textbox").fill("security log");
    await palette.getByRole("option", { name: /Security log/ }).click();
    await expect(page).toHaveURL(/#\/customize\/security_event/);
    await expect(page.getByRole("tab", { name: "Security", selected: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Security log", exact: true })).toBeInViewport();
  });
});

test("a switch that gets focus in an open panel does not scroll the page (B1)", async ({
  page,
  signIn,
  scratchModule,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  // Enough fields that the form is taller than the window, the switch last.
  const fields = [
    { name: "title", required: true },
    ...Array.from({ length: 14 }, (_, i) => ({ name: `note_${i + 1}` })),
    { name: "signature_required", kind: "bool" as const },
  ];
  const model = await scratchModule({ title: "Switch jump", fields });
  await signIn("root", `/${model}/${model}?id=new`);
  const panel = page.getByRole("complementary", { name: /record$/ });
  await expect(panel).toBeVisible();
  const sw = panel.getByRole("switch");
  await sw.focus();
  await expect(sw).toBeFocused();
  const after = await page.evaluate(() => ({
    scrollHeight: document.documentElement.scrollHeight,
    innerHeight,
    scrollY,
  }));
  expect(after.scrollY).toBe(0);
  expect(after.scrollHeight).toBe(after.innerHeight);
});

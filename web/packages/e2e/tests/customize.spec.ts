import { expect, test } from "../support/fixtures.ts";

test.describe("Customize: marketplace, module builder and theme", () => {
  test("install a marketplace module, use its app, then uninstall it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const plate = uid("PL").replace(/ /g, "-");
    try {
      await signIn("root", "/customize/modules?tab=modules&view=marketplace");
      const card = page.getByRole("article", { name: "Fleet", exact: true });
      await expect(card.getByRole("button", { name: "Install" })).toBeEnabled();
      await card.getByRole("button", { name: "Install" }).click();
      // The plan dialog says what will be installed; one confirmation applies it.
      const plan = page.getByRole("dialog", { name: "Install Fleet" });
      await expect(plan.getByRole("listitem").first()).toContainText("Fleet");
      await plan.getByRole("button", { name: "Install", exact: true }).click();
      await expect(page.locator(".toast").filter({ hasText: "Installed Fleet" })).toBeVisible();
      await expect(card.getByRole("button", { name: "Installed" })).toBeDisabled();

      // The new app is in the launcher and works like a built-in one.
      await page.getByRole("button", { name: "Open the app launcher" }).click();
      await page.getByRole("dialog", { name: "Apps" }).getByRole("link", { name: "Fleet" }).click();
      await expect(page).toHaveURL(/#\/fleet\/fleet_vehicle/);
      await page
        .getByRole("button", { name: /^(New|Create the first vehicle)$/ })
        .first()
        .click();
      const panel = page.getByRole("complementary", { name: "Vehicle record" });
      await panel.getByLabel("Plate").fill(plate);
      await panel.getByLabel("Odometer km").fill("120000");
      await panel.getByRole("button", { name: "Create" }).click();
      // Wait for the save: switching views while the form is still dirty would ask to discard it.
      await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
      await page.getByRole("button", { name: "List", exact: true }).click();
      await expect(page.getByRole("row", { name: new RegExp(plate) })).toContainText("120,000");

      // Uninstall from the Installed list; the app leaves the launcher.
      await page.goto("/#/customize/modules?tab=modules");
      const row = page.getByRole("article", { name: "Fleet", exact: true });
      await row.getByRole("button", { name: "Uninstall" }).click();
      await page.getByRole("alertdialog").getByRole("button", { name: "Uninstall" }).click();
      await expect(page.locator(".toast").filter({ hasText: "Removed Fleet" })).toBeVisible();
      await expect(row).toHaveCount(0);
      await page.getByRole("tab", { name: /^Marketplace/ }).click();
      await expect(card.getByRole("button", { name: "Install" })).toBeEnabled();
      await page.getByRole("button", { name: "Open the app launcher" }).click();
      await expect(
        page.getByRole("dialog", { name: "Apps" }).getByRole("link", { name: "Fleet" }),
      ).toHaveCount(0);
    } finally {
      await api.post("/api/module/uninstall", { id: "fleet" }, await api.token("root"));
    }
  });

  test("build a module with a choice, a link, an amount and a required field, install it and create a record", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Builder").replace(/ /g, "");
    const id = name.toLowerCase();
    const title = uid("First item");
    try {
      await signIn("root", "/customize/build?tab=build");
      await page.getByLabel("Module name").fill(name);
      await page.getByLabel("Record type").fill("Item");

      const rows = page.locator("li.fieldrow");
      // Row 0 stays the required Title; turn the others into a choice and an amount, add a link.
      await rows.nth(1).getByLabel("Field name").fill("priority");
      await rows.nth(1).getByLabel("Field kind").selectOption("select");
      await rows.nth(1).getByLabel("Options").fill("low, high");
      await rows.nth(2).getByLabel("Field name").fill("price");
      await rows.nth(2).getByLabel("Field kind").selectOption("money");
      await page.getByRole("button", { name: "Add field" }).click();
      await rows.nth(3).getByLabel("Field name").fill("customer");
      await rows.nth(3).getByLabel("Field kind").selectOption("ref");
      await rows.nth(3).getByLabel("Target model").selectOption("party");
      await expect(rows.nth(2).getByText("stored as price_minor")).toBeVisible();
      await expect(rows.nth(3).getByText("stored as customer_id")).toBeVisible();

      // The generated manifest is shown and reflects the choices.
      const toml = page.locator("pre.code");
      await expect(toml).toContainText('kind = "select"');
      await expect(toml).toContainText('options = ["low", "high"]');
      await expect(toml).toContainText('name = "price_minor"');
      await expect(toml).toContainText('to = "party"');
      await expect(toml).toContainText("required = true");

      await page.getByRole("button", { name: "Create and install" }).click();
      await expect(page.locator(".toast").filter({ hasText: `Installed ${name}` })).toBeVisible();
      await expect(page).toHaveURL(new RegExp(`#/${id}(/|$)`));

      // Use it: the required field is enforced, then every field type saves.
      await page
        .getByRole("button", { name: /^(New|Create the first item)$/ })
        .first()
        .click();
      const panel = page.getByRole("complementary", { name: "Item record" });
      await panel.getByRole("button", { name: "Create" }).click();
      await expect(panel.getByRole("alert").filter({ hasText: "Title is required" })).toBeVisible();
      await panel.getByLabel("Title").fill(title);
      await panel.getByLabel("Priority").selectOption("high");
      await panel.getByLabel("Price").fill("12.50");
      await panel.getByLabel("Customer").click();
      await page.getByRole("option", { name: "Ada Example" }).click();
      await panel.getByRole("button", { name: "Create" }).click();
      await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();

      await page.getByRole("button", { name: "List", exact: true }).click();
      const row = page.getByRole("row", { name: new RegExp(title) });
      await expect(row).toContainText("High");
      await expect(row).toContainText("€12.50");
      await expect(row).toContainText("Ada Example");
      const stored = (await api.list("root", `${id}_item`)).find((r) => r.title === title);
      expect(stored).toMatchObject({ priority: "high", price_minor: 1250 });
    } finally {
      await api.post("/api/module/uninstall", { id }, await api.token("root"));
    }
  });

  test("the builder refuses an incomplete module and says why", async ({ page, signIn }) => {
    await signIn("root", "/customize/build?tab=build");
    await page.getByLabel("Module name").fill("   ");
    await page.getByRole("button", { name: "Create and install" }).click();
    await expect(page.getByRole("alert")).toHaveText("Give the module a name");
    await page.getByLabel("Module name").fill("Needs a target");
    await page.getByRole("button", { name: "Add field" }).click();
    await page.locator("li.fieldrow").last().getByLabel("Field name").fill("link");
    await page.locator("li.fieldrow").last().getByLabel("Field kind").selectOption("ref");
    await page.getByRole("button", { name: "Create and install" }).click();
    await expect(page.getByRole("alert")).toContainText(
      "Pick the record type that link_id links to",
    );
  });

  test("theme changes apply live, 'Save for me' survives a reload, and 'Reset mine' goes back", async ({
    page,
    signIn,
  }) => {
    await signIn("root", "/customize/theme?tab=theme");
    const html = page.locator("html");
    await page.getByRole("button", { name: "Teal" }).click();
    await expect(html).toHaveAttribute("style", /--accent: #0b8a8f/);
    await page.getByRole("radio", { name: "Dark" }).click();
    await expect(html).toHaveAttribute("data-theme", "dark");
    await page.getByRole("radio", { name: "Round" }).click();
    await expect(html).toHaveAttribute("style", /--radius: 20px/);
    await page.getByRole("radio", { name: "Compact" }).click();
    await expect(html).toHaveAttribute("data-density", "compact");

    // Live only: a reload without saving drops it.
    await page.reload();
    await expect(html).not.toHaveAttribute("data-theme", "dark");

    await page.getByRole("button", { name: "Teal" }).click();
    await page.getByRole("radio", { name: "Dark" }).click();
    await page.getByRole("button", { name: "Save for me" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Saved for you" })).toBeVisible();
    await page.reload();
    await expect(html).toHaveAttribute("style", /--accent: #0b8a8f/);
    await expect(html).toHaveAttribute("data-theme", "dark");

    await page.getByRole("button", { name: "Reset mine" }).click();
    await expect(
      page.locator(".toast").filter({ hasText: "Back to the instance default" }),
    ).toBeVisible();
    await page.reload();
    await expect(html).not.toHaveAttribute("data-theme", "dark");
    await expect(html).toHaveAttribute("style", /--accent: #6d3fd0/);
  });

  test("'Set for everyone' as admin changes the theme a different user gets after signing in", async ({
    page,
    browser,
    signIn,
    api,
  }) => {
    try {
      await signIn("root", "/customize/theme?tab=theme");
      await page.getByRole("button", { name: "Rose" }).click();
      await page.getByRole("radio", { name: "Sharp" }).click();
      await page.getByRole("button", { name: "Set for everyone" }).click();
      await expect(
        page.locator(".toast").filter({ hasText: "Set as the default for everyone" }),
      ).toBeVisible();

      const ctx = await browser.newContext({ locale: "en-US" });
      const sam = await ctx.newPage();
      const problems: string[] = [];
      sam.on("pageerror", (e) => problems.push(e.message));
      await sam.goto("/");
      await sam.getByLabel("Username").fill("sam");
      await sam.getByLabel("Password").fill("e2e sam password 1");
      await sam.getByRole("button", { name: "Sign in" }).click();
      await expect(sam.locator(".topbar")).toBeVisible();
      await expect(sam.locator("html")).toHaveAttribute("style", /--accent: #d6336c/);
      await expect(sam.locator("html")).toHaveAttribute("style", /--radius: 4px/);
      // A non-admin has no Customize app and cannot change the instance default.
      await expect(sam.getByRole("link", { name: "Customize" })).toHaveCount(0);
      expect(problems).toEqual([]);
      await ctx.close();
    } finally {
      await api.run("root", "setting.update", { id: "instance", value: "{}" });
    }
  });
});

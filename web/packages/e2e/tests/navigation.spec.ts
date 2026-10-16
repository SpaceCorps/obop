import { expect, test } from "../support/fixtures.ts";

test.describe("launcher and command palette", () => {
  test("the launcher lists the apps, filters them and opens one", async ({ page, signIn }) => {
    await signIn("root");
    await page.getByRole("button", { name: "Open the app launcher" }).click();
    const launcher = page.getByRole("dialog", { name: "Apps" });
    for (const app of ["Contacts", "Products", "CRM", "Sales", "Invoicing", "Customize"])
      await expect(launcher.getByRole("link", { name: app })).toBeVisible();

    await launcher.getByLabel("Search apps").fill("crm");
    await expect(launcher.getByRole("link")).toHaveCount(1);
    await launcher.getByRole("link", { name: "CRM" }).click();

    await expect(launcher).toHaveCount(0);
    await expect(page).toHaveURL(/#\/crm\/lead$/);
    await expect(page.getByRole("navigation", { name: "CRM menu" })).toBeVisible();
    await expect(page.getByRole("region", { name: "Lead" })).toBeVisible();
  });

  test("Escape closes the launcher without navigating", async ({ page, signIn }) => {
    await signIn("root");
    await page.getByRole("button", { name: "Open the app launcher" }).click();
    await expect(page.getByRole("dialog", { name: "Apps" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: "Apps" })).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
  });

  test("Ctrl/Cmd+K opens the palette; typing and Enter jump to a list", async ({ page, signIn }) => {
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await expect(palette).toBeVisible();
    await expect(palette.getByRole("textbox")).toBeFocused();

    await palette.getByRole("textbox").fill("sale order");
    await expect(palette.getByRole("option", { name: /Sales › Sale order/ })).toBeVisible();
    await page.keyboard.press("Enter");

    await expect(palette).toHaveCount(0);
    await expect(page).toHaveURL(/#\/sales\/sale_order$/);
    await expect(page.getByRole("region", { name: "Sale order" })).toBeVisible();

    // Toggles closed with the same shortcut.
    await page.keyboard.press("ControlOrMeta+k");
    await expect(palette).toBeVisible();
    await page.keyboard.press("ControlOrMeta+k");
    await expect(palette).toHaveCount(0);
  });

  test("the palette moves with the arrow keys and creates records", async ({ page, signIn }) => {
    await signIn("root", "/crm/lead");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("textbox").fill("new lead");
    const hit = palette.getByRole("option", { name: /New lead/ });
    await expect(hit).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowUp");
    await expect(hit).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("complementary", { name: "Lead record" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "New lead" })).toBeVisible();
    await expect(page).toHaveURL(/id=new/);
  });

  test("the palette says so when nothing matches, and the theme command opens Customize", async ({ page, signIn }) => {
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("textbox").fill("zzzzzz");
    await expect(palette.getByText("Nothing found.")).toBeVisible();
    await palette.getByRole("textbox").fill("change theme");
    await palette.getByRole("option", { name: /Change theme/ }).click();
    await expect(page).toHaveURL(/#\/customize\/theme/);
    await expect(page.getByRole("heading", { name: "Colour" })).toBeVisible();
  });
});

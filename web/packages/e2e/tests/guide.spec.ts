import { expect, open, test } from "../support/fixtures.ts";

// "How to write a module" is docs/MODULES.md, imported at build time and drawn in Customize > Guide.
const GOVERNED = "61-governed-fields-actions-are-the-only-way-to-move-a-state-d48";

test.describe("module guide", () => {
  test("renders the document with a table of contents that jumps to a section and keeps it in the address", async ({
    page,
    signIn,
  }) => {
    await signIn("root", "/customize/guide?tab=guide");
    await expect(page.getByRole("tab", { name: "Guide", selected: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "How to write a module" })).toBeVisible();

    const toc = page.getByRole("navigation", { name: "Contents" });
    expect(await toc.getByRole("link").count()).toBeGreaterThan(25);
    await toc.getByRole("link", { name: /^6\.1 Governed fields/ }).click();
    await expect(page).toHaveURL(new RegExp(`section=${GOVERNED}`));
    const heading = page.locator(`#g-${GOVERNED}`);
    await expect(heading).toBeInViewport();
    await expect(heading).toBeFocused();
    await expect(toc.getByRole("link", { name: /^6\.1 Governed fields/ })).toHaveAttribute(
      "aria-current",
      "location",
    );

    // Tables and lists come out as real tables and lists.
    await expect(page.getByRole("table").first()).toBeVisible();
    expect(await page.locator(".guide-text li").count()).toBeGreaterThan(20);
  });

  test("an address with a section opens at that section, and 'section 6.1' in the text is a link to it", async ({
    page,
    signIn,
  }) => {
    await signIn("root", `/customize/guide?tab=guide&section=${GOVERNED}`);
    await expect(page.locator(`#g-${GOVERNED}`)).toBeInViewport();

    // From the top, the reference in the introduction of section 2.2 leads to 6.1.
    await open(page, "/customize/guide?tab=guide");
    const ref = page.locator(".guide-text a", { hasText: /^6\.1$/ }).first();
    await ref.scrollIntoViewIfNeeded();
    await ref.click();
    await expect(page).toHaveURL(new RegExp(`section=${GOVERNED}`));
    await expect(page.locator(`#g-${GOVERNED}`)).toBeInViewport();
  });

  test("search lists the sections that match, opens one, and says so when nothing matches", async ({
    page,
    signIn,
  }) => {
    await signIn("root", "/customize/guide?tab=guide");
    await expect(page.getByRole("heading", { name: "How to write a module" })).toBeVisible();
    const box = page.getByRole("searchbox", { name: "Search the guide" });
    await page.keyboard.press("/");
    await expect(box).toBeFocused();
    await box.fill("approval");
    const results = page.getByRole("region", { name: "Search results" });
    await expect(results.getByRole("heading")).toHaveText(/\d+ sections match “approval”/);
    const hits = results.getByRole("listitem");
    expect(await hits.count()).toBeGreaterThan(2);
    await expect(hits.first().locator("p")).toContainText(/approv/i);

    await results.getByRole("link", { name: /^6\.1 Governed fields/ }).click();
    await expect(box).toHaveValue("");
    await expect(page).toHaveURL(new RegExp(`section=${GOVERNED}`));
    await expect(page.locator(`#g-${GOVERNED}`)).toBeInViewport();

    await box.fill("zzzzqqqq");
    await expect(page.getByRole("heading", { name: "Nothing matches" })).toBeVisible();
    await box.press("Escape");
    await expect(box).toHaveValue("");
    await expect(page.getByRole("heading", { name: "How to write a module" })).toBeVisible();
  });

  test("code blocks are highlighted TOML and can be copied", async ({ page, signIn, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await signIn(
      "root",
      "/customize/guide?tab=guide&section=23-the-main-model-kinds-refs-a-field-level-group-personal-data",
    );
    const block = page.locator(".codeblock[data-lang='toml']", { hasText: "assets_asset" }).first();
    await block.scrollIntoViewIfNeeded();
    await expect(block.locator(".tok-table").first()).toHaveText("[[model]]");
    await expect(block.locator(".tok-key").first()).toBeVisible();
    await expect(block.locator(".tok-string").first()).toContainText('"');

    await block.getByRole("button", { name: "Copy" }).click();
    await expect(block.getByRole("button", { name: "Copied" })).toBeVisible();
    const copied = await page.evaluate(() => navigator.clipboard.readText());
    expect(copied.startsWith("[[model]]")).toBe(true);
    expect(copied).toContain('name = "assets_asset"');
  });

  test("it is linked from the Modules tab, the module builder and the command palette", async ({
    page,
    signIn,
  }) => {
    await signIn("root", "/customize/modules?tab=modules");
    await page.getByRole("link", { name: "How to write a custom module?" }).click();
    await expect(page).toHaveURL(/#\/customize\/guide\?tab=guide/);
    await expect(page.getByRole("heading", { name: "How to write a module" })).toBeVisible();

    await open(page, "/customize/build?tab=build");
    await page.getByRole("link", { name: "How to write a custom module?" }).click();
    await expect(page).toHaveURL(/#\/customize\/guide\?tab=guide/);

    await open(page, "/");
    await page.keyboard.press("ControlOrMeta+k");
    await page
      .getByRole("combobox", { name: "Search apps, records and actions" })
      .or(page.getByRole("textbox", { name: "Search apps, records and actions" }))
      .fill("write a module");
    await page.getByRole("option", { name: /How to write a custom module/ }).click();
    await expect(page).toHaveURL(/#\/customize\/guide\?tab=guide/);
  });

  test("an ordinary user can read the guide too", async ({ page, signIn }) => {
    await signIn("sam", "/customize/guide?tab=guide");
    await expect(page.getByRole("heading", { name: "How to write a module" })).toBeVisible();
    // The administrator tabs stay hidden from them.
    await expect(page.getByRole("tab", { name: "Modules" })).toHaveCount(0);
    await expect(page.getByRole("tab", { name: "Guide", selected: true })).toBeVisible();
  });
});

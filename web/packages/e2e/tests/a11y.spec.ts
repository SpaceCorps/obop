import { expect, open, test, unnamedControls } from "../support/fixtures.ts";

// A small DOM check, no extra dependency: every visible button, input, select, textarea and link
// must have an accessible name. (Console and page errors are checked in every test by the fixtures.)
test.describe("accessibility smoke", () => {
  test("the sign-in form and every main view have no unlabeled controls", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
    expect(await unnamedControls(page), "sign-in").toEqual([]);

    const model = await scratchModule({
      title: "Smoke data",
      fields: [
        { name: "title", required: true },
        { name: "state", kind: "select", options: ["a", "b"] },
        { name: "amount_minor", kind: "money" },
        { name: "due", kind: "date" },
      ],
    });
    await api.create("root", model, {
      title: uid("Smoke"),
      state: "a",
      amount_minor: 100,
      due: "2020-01-01",
    });
    await signIn("root");
    expect(await unnamedControls(page), "home").toEqual([]);

    for (const view of ["list", "kanban", "calendar", "pivot", "graph"]) {
      await open(page, `/${model}/${model}?view=${view}`);
      await expect(page.locator(`.content.view-${view}`)).toBeVisible();
      await expect(page.locator(".content .skeleton")).toHaveCount(0);
      expect(await unnamedControls(page), `view ${view}`).toEqual([]);
    }

    await open(page, `/${model}/${model}?view=list`);
    await page.getByRole("button", { name: "Filters, group by and favorites" }).click();
    await page.getByRole("button", { name: "Add custom filter" }).click();
    expect(await unnamedControls(page), "filter panel").toEqual([]);
    await page.keyboard.press("Escape");

    await page.getByRole("button", { name: "Open record" }).first().click();
    await expect(page.getByRole("complementary", { name: /record$/ })).toBeVisible();
    expect(await unnamedControls(page), "record panel").toEqual([]);

    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Open the app launcher" }).click();
    expect(await unnamedControls(page), "launcher").toEqual([]);
  });

  // BUG (not fixed here): the command palette input (Chrome.tsx, Palette) has only a placeholder, no
  // aria-label or label, so screen readers announce an unnamed edit field.
  // Reproduce: sign in, press Ctrl/Cmd+K, inspect the focused input.
  test("the command palette input has an accessible name", async ({ page, signIn }) => {
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    await expect(page.getByRole("dialog", { name: "Command palette" })).toBeVisible();
    expect(await unnamedControls(page), "palette").toEqual([]);
  });

  test("every Customize tab has no unlabeled controls", async ({ page, signIn }) => {
    await signIn("root");
    for (const tab of ["theme", "apps", "modules", "build", "guide"]) {
      await open(page, `/customize/${tab}?tab=${tab}`);
      // The Modules tab has tabs of its own (Installed, Marketplace, Updates): the first is Customize's.
      await expect(page.getByRole("tab", { name: /./, selected: true }).first()).toBeVisible();
      if (tab === "modules") await expect(page.locator("article.card").first()).toBeVisible();
      expect(await unnamedControls(page), `customize ${tab}`).toEqual([]);
    }
  });

  test("the control plane sign-up and instances pages have no unlabeled controls", async ({
    page,
  }) => {
    const { platformUrl } = await import("../support/env.ts");
    await page.goto(`${platformUrl()}/app/`);
    await expect(page.getByRole("button", { name: "Create account" })).toBeVisible();
    expect(await unnamedControls(page), "platform sign-up").toEqual([]);
    await page.goto(`${platformUrl()}/`);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    expect(await unnamedControls(page), "landing").toEqual([]);
  });
});

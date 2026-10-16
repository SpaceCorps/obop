import type { Locator, Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";

// A board over a throwaway module, so every count below is the only data there is.
const STAGES = ["new", "qualified", "won", "lost"];
const fields = [
  { name: "title", required: true },
  { name: "state", kind: "select" as const, options: STAGES },
  { name: "amount_minor", kind: "money" as const },
];

const board = (page: Page) => page.getByRole("group", { name: /board by State/ });
const column = (page: Page, stage: string) =>
  board(page).getByRole("region", { name: new RegExp(`^${stage}, \\d+ cards?$`, "i") });
const card = (col: Locator, title: string) => col.locator(".kcard", { hasText: title });
const stateOf = async (api: Api, model: string, title: string) =>
  (await api.list("root", model)).find((r) => r.title === title)?.state;

test.describe("kanban board", () => {
  test("dragging a card to another stage moves it and updates the record and the column totals", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ title: "Drag board", fields });
    const [a, b] = [uid("Alpha"), uid("Beta")];
    await api.create("root", model, { title: a, state: "new", amount_minor: 1000 });
    await api.create("root", model, { title: b, state: "new", amount_minor: 2500 });
    await signIn("root", `/${model}/${model}?view=kanban`);

    await expect(column(page, "New").locator(".kcard")).toHaveCount(2);
    await expect(column(page, "New").locator(".sum")).toHaveText("€35.00");

    // Drop on the column header: the drag source and target are both on screen for any board size.
    await card(column(page, "New"), a).dragTo(column(page, "Won").locator("header"));
    await expect(card(column(page, "Won"), a)).toHaveCount(1);
    await expect(column(page, "New").locator(".kcard")).toHaveCount(1);
    await expect(column(page, "Won").locator(".sum")).toHaveText("€10.00");
    await expect(column(page, "New").locator(".sum")).toHaveText("€25.00");
    expect(await stateOf(api, model, a)).toBe("won");
    expect(await stateOf(api, model, b)).toBe("new");

    // And back again, onto the body of the column this time.
    await card(column(page, "Won"), a).dragTo(column(page, "Qualified"));
    await expect(card(column(page, "Qualified"), a)).toHaveCount(1);
    expect(await stateOf(api, model, a)).toBe("qualified");
  });

  test("a card added with the quick form can be dragged at once", async ({ page, signIn, api, uid, scratchModule }) => {
    const model = await scratchModule({ title: "Quick board", fields });
    const title = uid("Quick");
    await api.create("root", model, { title: uid("Seed"), state: "new" });
    await signIn("root", `/${model}/${model}?view=kanban`);

    await page.getByRole("button", { name: "Add a card to New" }).click();
    await page.getByLabel("New title").fill(title);
    await page.keyboard.press("Enter");
    await expect(card(column(page, "New"), title)).toHaveCount(1);

    await card(column(page, "New"), title).dragTo(column(page, "Lost").locator("header"));
    await expect(card(column(page, "Lost"), title)).toHaveCount(1);
    expect(await stateOf(api, model, title)).toBe("lost");
  });

  test("the Move to select and Shift+Arrow keys are the keyboard twin of dragging", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ title: "Keys board", fields });
    const title = uid("Keyed");
    await api.create("root", model, { title, state: "new" });
    await signIn("root", `/${model}/${model}?view=kanban`);

    const cardIn = (stage: string) => card(column(page, stage), title);
    await cardIn("New").getByLabel(/^Move .* to$/).selectOption({ label: "Lost" });
    await expect(cardIn("Lost")).toHaveCount(1);
    expect(await stateOf(api, model, title)).toBe("lost");

    // Shift+ArrowLeft moves the card one column back and focus stays on it.
    await cardIn("Lost").focus();
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(cardIn("Won")).toHaveCount(1);
    await expect(cardIn("Won")).toBeFocused();
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(cardIn("Qualified")).toBeFocused();
    expect(await stateOf(api, model, title)).toBe("qualified");

    // Plain arrows only move focus; Enter opens the record.
    await page.keyboard.press("ArrowRight");
    await expect(cardIn("Qualified")).toHaveCount(1);
    await page.keyboard.press("Enter");
    await expect(page.getByRole("complementary", { name: /record$/ })).toBeVisible();
  });

  test("a tall board still accepts a drop: events with a shared DataTransfer, no pointer geometry", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    // Diagnosis of the old flaky drag: a long column pushes the other columns off screen, and a
    // pointer drag needs source and target on screen together. This drives the same handlers the
    // browser would (dragstart, dragover, drop) without any pointer, so it does not depend on scroll.
    const model = await scratchModule({ title: "Tall board", fields });
    const title = uid("Far");
    await api.create("root", model, { title, state: "new" });
    for (let i = 0; i < 30; i++) await api.create("root", model, { title: uid("Filler"), state: "won" });
    await signIn("root", `/${model}/${model}?view=kanban`);

    await expect(column(page, "Won").locator(".kcard")).toHaveCount(30);
    const source = card(column(page, "New"), title);
    const target = column(page, "Won");
    const data = await page.evaluateHandle(() => new DataTransfer());
    await source.dispatchEvent("dragstart", { dataTransfer: data });
    await target.dispatchEvent("dragover", { dataTransfer: data });
    await target.dispatchEvent("drop", { dataTransfer: data });

    await expect(card(column(page, "Won"), title)).toHaveCount(1);
    expect(await stateOf(api, model, title)).toBe("won");
  });

  test("a column folds away and stays folded after a reload", async ({ page, signIn, api, uid, scratchModule }) => {
    const model = await scratchModule({ title: "Fold board", fields });
    await api.create("root", model, { title: uid("Folded"), state: "lost" });
    await signIn("root", `/${model}/${model}?view=kanban`);

    await expect(column(page, "Lost").locator(".kcard")).toHaveCount(1);
    await page.getByRole("button", { name: "Fold Lost" }).click();
    await expect(column(page, "Lost").locator(".kcard")).toHaveCount(0);
    await page.reload();
    await expect(page.getByRole("button", { name: "Unfold Lost" })).toBeVisible();
    await page.getByRole("button", { name: "Unfold Lost" }).click();
    await expect(column(page, "Lost").locator(".kcard")).toHaveCount(1);
  });
});

import type { Locator, Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";
import type { ModuleSpec } from "../support/modules.ts";

// Governed workflows (D48): `state` is moved by actions, so the UI offers exactly the moves the
// server accepts. The module has start (new to doing), finish (doing to done) and reopen (done to
// new, which needs a human's approval when an agent runs it).
const spec = (title: string): Omit<ModuleSpec, "id"> => ({
  title,
  fields: [
    { name: "title", required: true },
    { name: "state", kind: "select", options: ["new", "doing", "done"] },
  ],
  actions: [
    { name: "start", field: "state", from: "new", to: "doing" },
    { name: "finish", field: "state", from: "doing", to: "done", approval: true },
    { name: "reopen", field: "state", from: "done", to: "new", approval: true },
  ],
});

const panelOf = (page: Page) => page.getByRole("complementary", { name: /record$/ });
const bar = (page: Page) => panelOf(page).getByRole("list", { name: "State" });
/** The step the record is at: the status bar is a stepper that only shows the way (record form). */
const current = (page: Page) => bar(page).locator("li[aria-current=step]");
const actionRow = (page: Page) => panelOf(page).locator(".actionrow");
const action = (page: Page, name: string | RegExp) => actionRow(page).getByRole("button", { name });
const stateOf = async (api: Api, model: string, id: string) =>
  (await api.run("root", `${model}.get`, { id })).state;

test.describe("governed fields in the UI", () => {
  test("creating shows the first state read-only; the server starts the record there", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule(spec("Governed create"));
    const title = uid("Fresh");
    await signIn("root", `/${model}/${model}?view=list&id=new`);
    const panel = panelOf(page);
    await expect(current(page)).toHaveText("New");
    // No step can be pressed: a governed state moves only through actions.
    await expect(bar(page).getByRole("button")).toHaveCount(0);
    await expect(panel.locator(".rec-starts")).toContainText(
      `A new ${model} starts as New; it moves on through actions`,
    );

    await panel.getByLabel("Title").fill(title);
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    const stored = (await api.list("root", model)).find((r) => r.title === title);
    expect(stored?.state).toBe("new");
  });

  test("the action row offers only the moves valid now, the next step first", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule(spec("Governed bar"));
    const rec = await api.create("root", model, { title: uid("Bar"), state: "new" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);

    await expect(current(page)).toHaveText("New");
    // Only Start leads on from New: it is the one filled button, and no other move is offered.
    await expect(actionRow(page).getByRole("button")).toHaveCount(1);
    await expect(action(page, "Start")).toHaveClass(/primary/);

    await action(page, "Start").click();
    await expect(page.locator(".toast").filter({ hasText: "is now doing" })).toBeVisible();
    expect(await stateOf(api, model, rec.id)).toBe("doing");
    await expect(current(page)).toHaveText("Doing");

    // Finish needs a person's approval when an agent runs it: it carries the shield, not a star.
    const finish = action(page, /^Finish/);
    await expect(finish).toHaveClass(/primary/);
    await expect(finish.locator(".approval-mark")).toHaveAttribute(
      "aria-label",
      "An agent needs a person's approval for this",
    );
    await finish.click();
    await expect(page.locator(".toast").filter({ hasText: "is now done" })).toBeVisible();
    expect(await stateOf(api, model, rec.id)).toBe("done");

    // Only Reopen is available now, and a move back is not the next step.
    await expect(action(page, /^Reopen/)).toBeVisible();
    await expect(action(page, /^Reopen/)).not.toHaveClass(/primary/);
    await expect(action(page, "Start")).toHaveCount(0);
  });

  test("a stale page that tries a move the server now refuses shows the server's message", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule(spec("Governed stale"));
    const rec = await api.create("root", model, { title: uid("Stale"), state: "new" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    await expect(action(page, "Start")).toBeVisible();

    // Someone else moves it on while this page is open.
    await api.run("root", `${model}.start`, { id: rec.id });
    await action(page, "Start").click();
    const alert = panelOf(page).getByRole("alert");
    await expect(alert).toBeVisible();
    await expect(alert).toContainText(/new/);
    await expect(page.locator(".toast").filter({ hasText: /new/ })).toBeVisible();
    expect(await stateOf(api, model, rec.id)).toBe("doing");
  });

  test("kanban: a drag to a state no action reaches is refused, an allowed one runs the action", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule(spec("Governed board"));
    const title = uid("Card");
    const rec = await api.create("root", model, { title, state: "new" });
    await signIn("root", `/${model}/${model}?view=kanban`);
    const board = page.getByRole("group", { name: /board by State/ });
    const col = (name: string): Locator =>
      board.getByRole("region", { name: new RegExp(`^${name}, \\d+ cards?$`, "i") });
    const card = (name: string) => col(name).locator(".kcard", { hasText: title });

    // New to Done has no action: the drop is not accepted, nothing moves, nothing is sent.
    await card("New").dragTo(col("Done").locator("header"));
    await expect(card("New")).toHaveCount(1);
    await expect(card("Done")).toHaveCount(0);
    expect(await stateOf(api, model, rec.id)).toBe("new");

    // The card's Move to menu lists every other state, the unreachable ones disabled and explained.
    await card("New")
      .getByRole("button", { name: /^Move .* to$/ })
      .click();
    const done = page.getByRole("menuitem", { name: /^Done/ });
    await expect(done).toBeDisabled();
    await expect(done).toHaveAttribute("title", `No action moves a ${model} from New to Done`);
    await expect(page.getByRole("menuitem", { name: /^Doing/ })).toBeEnabled();
    await page.keyboard.press("Escape");
    // The keyboard twin (Shift+Arrow) follows the same rules: one step right is allowed, the next is not.
    await card("New").focus();
    await page.keyboard.press("Shift+ArrowRight");
    await expect(card("Doing")).toHaveCount(1);
    expect(await stateOf(api, model, rec.id)).toBe("doing");

    // From Doing, finish is a declared action: the drop is accepted and the state follows.
    await card("Doing").dragTo(col("Done").locator("header"));
    await expect(card("Done")).toHaveCount(1);
    expect(await stateOf(api, model, rec.id)).toBe("done");
  });

  test("a governed field that is not the status shows its value with a Move to menu", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Governed phase",
      fields: [
        { name: "title", required: true },
        { name: "phase", kind: "select", options: ["draft", "review", "final"] },
      ],
      actions: [
        { name: "submit", field: "phase", from: "draft", to: "review" },
        { name: "sign_off", field: "phase", from: "review", to: "final", approval: true },
      ],
    });
    const rec = await api.create("root", model, { title: uid("Phase"), phase: "draft" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    const panel = page.getByRole("complementary", { name: /record$/ });
    await panel.getByRole("button", { name: "Move phase" }).click();
    const final = page.getByRole("menuitem", { name: /Final/ });
    await expect(final).toBeDisabled();
    await expect(final).toContainText(`No action moves a ${model} from Draft to Final`);
    await page.getByRole("menuitem", { name: "Review" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Phase: Review" })).toBeVisible();
    expect((await api.run("root", `${model}.get`, { id: rec.id })).phase).toBe("review");
  });
});

test.describe("governed fields for an agent", () => {
  test("a move that needs approval is proposed: the toast names the plan and nothing changes", async ({
    page,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule({ ...spec("Governed agent"), id: "wfagent" });
    const rec = await api.create("root", model, { title: uid("Agent") });
    await api.run("root", `${model}.start`, { id: rec.id });
    // A person who holds the module's groups delegates them to an agent (a token only narrows its owner).
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: ["wfagent.user", "wfagent.manager"], label: "workflow agent" },
      await api.token("ivy"),
    );
    expect(issued.status).toBe(200);
    await page.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [issued.json.token, "agent"],
    );
    await page.goto(`/#/${model}/${model}?view=list&id=${rec.id}`);

    // The action proposes: the toast names the plan and nothing changes.
    await action(page, /^Finish/).click();
    const toast = page.locator(".toast").filter({ hasText: "Approval needed" });
    await expect(toast).toContainText("Finish");
    await expect(toast).toContainText(/plan [0-9a-f]{8}/);
    await expect(toast).toContainText("Nothing changes until a human approves it");
    expect(await stateOf(api, model, rec.id)).toBe("doing");
  });
});

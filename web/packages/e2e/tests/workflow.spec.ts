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

const bar = (page: Page) =>
  page.getByRole("complementary", { name: /record$/ }).getByRole("list", { name: "State" });
const step = (page: Page, name: string) => bar(page).getByRole("button", { name, exact: true });
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
    const panel = page.getByRole("complementary", { name: /record$/ });
    await expect(step(page, "New")).toHaveAttribute("aria-current", "step");
    for (const other of ["Doing", "Done"]) {
      await expect(step(page, other)).toHaveAttribute("aria-disabled", "true");
      await step(page, other).click({ force: true }); // a click on a blocked step does nothing
      await expect(step(page, "New")).toHaveAttribute("aria-current", "step");
    }
    await step(page, "Done").hover();
    await expect(page.getByRole("tooltip")).toContainText(
      `A new ${model} starts as New; it moves on through actions`,
    );

    await panel.getByLabel("Title").fill(title);
    await panel.getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    const stored = (await api.list("root", model)).find((r) => r.title === title);
    expect(stored?.state).toBe("new");
  });

  test("the status bar enables only the moves an action allows and explains the others", async ({
    page,
    signIn,
    api,
    uid,
    scratchModule,
  }) => {
    const model = await scratchModule(spec("Governed bar"));
    const rec = await api.create("root", model, { title: uid("Bar"), state: "new" });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);

    await expect(step(page, "New")).toHaveAttribute("aria-current", "step");
    await expect(step(page, "Doing")).not.toHaveAttribute("aria-disabled", "true");
    await expect(step(page, "Done")).toHaveAttribute("aria-disabled", "true");
    await step(page, "Done").hover();
    await expect(page.getByRole("tooltip")).toHaveText(
      `No action moves a ${model} from New to Done`,
    );
    await step(page, "Done").click({ force: true });
    expect(await stateOf(api, model, rec.id)).toBe("new");

    // Doing runs the start action; Done is then reachable and shows the approval mark.
    await step(page, "Doing").click();
    await expect(page.locator(".toast").filter({ hasText: "State: Doing" })).toBeVisible();
    expect(await stateOf(api, model, rec.id)).toBe("doing");
    await expect(step(page, "Done *")).toBeVisible();
    await expect(step(page, "New")).toHaveAttribute("aria-disabled", "true");
    await step(page, "Done *").click();
    await expect(page.locator(".toast").filter({ hasText: "State: Done" })).toBeVisible();
    expect(await stateOf(api, model, rec.id)).toBe("done");

    // The action buttons follow the state too: only Reopen is available now.
    const actions = page.getByRole("complementary", { name: /record$/ });
    await expect(actions.getByRole("button", { name: "Reopen *" })).not.toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await expect(actions.getByRole("button", { name: "Start" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    await actions.getByRole("button", { name: "Start" }).hover();
    await expect(page.getByRole("tooltip")).toHaveText("Only available when State is New");
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
    await expect(step(page, "Doing")).not.toHaveAttribute("aria-disabled", "true");

    // Someone else moves it on while this page is open.
    await api.run("root", `${model}.start`, { id: rec.id });
    await step(page, "Doing").click();
    const alert = page.getByRole("complementary", { name: /record$/ }).getByRole("alert");
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

    // The Move to select lists every other state, with the unreachable ones disabled and explained.
    const moveTo = card("New").getByLabel(/^Move .* to$/);
    await expect(moveTo.locator("option", { hasText: "Done" })).toBeDisabled();
    await expect(moveTo.locator("option", { hasText: "Done" })).toHaveAttribute(
      "title",
      `No action moves a ${model} from New to Done`,
    );
    await expect(moveTo.locator("option", { hasText: "Doing" })).toBeEnabled();
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

    await expect(step(page, "Done *")).toBeVisible();
    await step(page, "Done *").click();
    const toast = page.locator(".toast").filter({ hasText: "Approval needed" });
    await expect(toast).toContainText("moving");
    await expect(toast).toContainText(/plan [0-9a-f]{8}/);
    await expect(toast).toContainText("Nothing changes until a human approves it");
    expect(await stateOf(api, model, rec.id)).toBe("doing");

    // The action button proposes in the same way.
    await page
      .getByRole("complementary", { name: /record$/ })
      .getByRole("button", { name: "Finish *" })
      .click();
    await expect(page.locator(".toast").filter({ hasText: "Approval needed" })).toHaveCount(2);
    expect(await stateOf(api, model, rec.id)).toBe("doing");
  });
});

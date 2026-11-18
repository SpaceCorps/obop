import type { Locator, Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";
import type { ModuleSpec } from "../support/modules.ts";

// The Approvals inbox (D97): an agent proposes a change that needs a person; the person sees the
// count in the bar, opens the plan on a phone, reads what changes, and approves or rejects it. Rule 10
// is held here in the real page: nothing decides from the list, the bar or a toast. The module is
// "wfagent" because the user `ivy` holds its groups and so can issue an agent token for it and is the
// person who decides what that agent proposes (an agent's own owner may decide, D97).
const MODEL = "wfagent";
const spec: ModuleSpec = {
  id: MODEL,
  title: "Approvals claims",
  fields: [
    { name: "title", required: true },
    { name: "amount", kind: "number" },
    { name: "state", kind: "select", options: ["new", "doing", "done"] },
  ],
  actions: [
    { name: "start", field: "state", from: "new", to: "doing" },
    { name: "finish", field: "state", from: "doing", to: "done", approval: true },
  ],
};

const PHONE = { width: 390, height: 844 };
const TOUCH = 44;

// The plan's panel is named by its own heading ("An agent asks to ..."), so it is found by what it
// is: beside the list an `aside`, over it (up to 1024 px) a dialog.
const panelOf = (page: Page) => page.locator("aside[data-plan-panel]:not([role])");
const dialogOf = (page: Page) => page.locator("aside[data-plan-panel][role=dialog]");

/**
 * The box of a control that must be on screen: its numbers, for the layout assertions. A sheet slides
 * in, so the box is measured where it comes to rest: after the animations of the panel that holds the
 * control (as `listProblems` waits for the page's), not while it still moves.
 */
async function boxOf(loc: Locator) {
  await expect(loc).toBeVisible();
  await loc.evaluate((el) =>
    Promise.all(
      (el.closest("aside") ?? el)
        .getAnimations({ subtree: true })
        .filter((a) => a.effect?.getComputedTiming().endTime !== Infinity)
        .map((a) => a.finished.catch(() => null)),
    ),
  );
  const b = await loc.boundingBox();
  expect(b, "a visible control has a box").not.toBeNull();
  return b!;
}

/** A control that a thumb can hit and that is entirely inside the screen. */
async function expectTappable(loc: Locator, viewport: { width: number; height: number }) {
  const b = await boxOf(loc);
  expect(b.height, "at least 44 px tall").toBeGreaterThanOrEqual(TOUCH - 0.5);
  expect(b.width, "at least 44 px wide").toBeGreaterThanOrEqual(TOUCH - 0.5);
  expect(b.x).toBeGreaterThanOrEqual(0);
  expect(b.y).toBeGreaterThanOrEqual(0);
  expect(b.x + b.width).toBeLessThanOrEqual(viewport.width + 0.5);
  expect(b.y + b.height).toBeLessThanOrEqual(viewport.height + 0.5);
}

type Setup = { agent: string; ivy: string; record: string; title: string };

/** An agent token of `ivy`, a record in `doing`, and what the agent proposes for it. */
async function propose(api: Api, uid: (p: string) => string): Promise<Setup & { plan: string }> {
  const ivy = await api.token("ivy");
  const issued = await api.post(
    "/api/auth/agent-token",
    { scopes: [`${MODEL}.user`, `${MODEL}.manager`], label: "approvals agent" },
    ivy,
  );
  expect(issued.status).toBe(200);
  const agent: string = issued.json.token;
  const title = uid("Chairs");
  const rec = await api.create("ivy", MODEL, { title, amount: 1200 });
  await api.run("ivy", `${MODEL}.start`, { id: rec.id });
  const plan = await proposeFinish(api, agent, rec.id);
  return { agent, ivy, record: rec.id, title, plan };
}

async function proposeFinish(api: Api, agent: string, id: string): Promise<string> {
  const r = await api.post(`/api/${MODEL}/finish`, { id }, agent);
  expect(r.status, JSON.stringify(r.json)).toBe(202);
  expect(typeof r.json.plan_id).toBe("string");
  return r.json.plan_id;
}

const waiting = async (api: Api, token: string): Promise<number> =>
  (await api.post("/api/plan/count", {}, token)).json.value.waiting;

test.describe("the approvals inbox on a phone", () => {
  test.use({ viewport: PHONE });

  test("a person sees the badge, reads what changes, approves; a second plan is rejected with a reason", async ({
    page,
    api,
    uid,
    scratchModule,
    signIn,
  }) => {
    await scratchModule(spec);
    const first = await propose(api, uid);
    const before = await waiting(api, first.ivy);
    expect(before).toBeGreaterThanOrEqual(1);

    // 1. The bar: the count, named in words, a link that only opens the page.
    await signIn("ivy");
    const entry = page.getByRole("link", { name: `Approvals, ${before} waiting for you` });
    await expect(entry).toBeVisible();
    await expectTappable(entry, PHONE);
    expect(await layoutProblems(page), "the bar at 390 px").toEqual([]);
    await entry.click();
    await expect(page).toHaveURL(/#\/approvals/);
    await expect(page.getByRole("heading", { level: 1, name: "Approvals" })).toBeVisible();

    // 2. Rule 10 on the list: no control that decides, one control per request that only opens it.
    const row = page.getByRole("button", { name: new RegExp(first.title) });
    await expect(row).toBeVisible();
    await expect(page.getByRole("button", { name: /^(Approve|Reject)/ })).toHaveCount(0);
    expect(await layoutProblems(page), "the list at 390 px").toEqual([]);
    await expect(page.locator(".appr-page")).not.toContainText(first.plan);

    // 3. The detail: what will happen is on the first screen, the footer is always in view.
    await row.click();
    const panel = dialogOf(page);
    await expect(panel).toBeVisible();
    await expect(panel.getByRole("heading", { level: 2 })).toContainText("An agent asks to");
    await expect(panel).toContainText("What changes");
    await expect(panel).toContainText(/State/);
    await expect(panel).toContainText(/Doing/);
    await expect(panel).toContainText(/Done/);
    const approve = panel.getByRole("button", { name: /^Approve and/ });
    const reject = panel.getByRole("button", { name: "Reject", exact: true });
    await expectTappable(approve, PHONE);
    await expectTappable(reject, PHONE);
    expect(await layoutProblems(page), "the detail at 390 px").toEqual([]);
    // Nothing has happened to the record yet.
    expect((await api.run("ivy", `${MODEL}.get`, { id: first.record })).state).toBe("doing");

    // 4. Approve: one press, the record changes, the plan leaves the waiting list.
    await approve.click();
    await expect(page.locator(".toast").filter({ hasText: "Approved." })).toBeVisible();
    expect((await api.run("ivy", `${MODEL}.get`, { id: first.record })).state).toBe("done");
    await expect(page.getByRole("button", { name: new RegExp(first.title) })).toHaveCount(0);
    const asAgent = await api.post("/api/plan/get", { id: first.plan }, first.agent);
    expect(asAgent.json.value.status).toBe("approved");

    // 5. A second plan is rejected with a reason, and the agent reads that.
    const title = uid("Desks");
    const rec = await api.create("ivy", MODEL, { title, amount: 300 });
    await api.run("ivy", `${MODEL}.start`, { id: rec.id });
    const second = await proposeFinish(api, first.agent, rec.id);
    await page.goto(`/#/approvals?id=${second}`);
    const again = dialogOf(page);
    await expect(again).toBeVisible();
    await again.getByRole("button", { name: "Reject", exact: true }).click();
    const reason = "Wrong cost centre, please send it again with the right one.";
    const ask = page.getByRole("alertdialog", { name: "Reject this request?" });
    await ask.getByLabel("Tell the agent why (optional)").fill(reason);
    await expect(ask).toContainText(`${reason.length} of 300 characters`);
    await ask.getByRole("button", { name: "Reject request" }).click();
    await expect(page.locator(".toast").filter({ hasText: "Rejected." })).toBeVisible();
    // Focus is on the page's heading after the decision, never left on `body` (the button that
    // opened the dialog is gone with the request).
    await expect(page.getByRole("heading", { level: 1, name: "Approvals" })).toBeFocused();
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("H1");
    expect((await api.run("ivy", `${MODEL}.get`, { id: rec.id })).state).toBe("doing");
    const told = await api.post("/api/plan/get", { id: second }, first.agent);
    expect(told.json.value.status).toBe("rejected");
    expect(told.json.value.reason).toBe(reason);
  });

  test("the toast of a change sent for approval has a link to the plan and no button that decides", async ({
    page,
    api,
    uid,
    scratchModule,
  }) => {
    await scratchModule(spec);
    const ivy = await api.token("ivy");
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: [`${MODEL}.user`, `${MODEL}.manager`], label: "page agent" },
      ivy,
    );
    expect(issued.status).toBe(200);
    const rec = await api.create("ivy", MODEL, { title: uid("Lamps") });
    await api.run("ivy", `${MODEL}.start`, { id: rec.id });
    const before = await waiting(api, ivy);
    // The page signed in with the agent's token: the person at the keyboard is the agent's own.
    await page.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [issued.json.token, "agent"],
    );
    await page.goto(`/#/${MODEL}/${MODEL}?view=list&id=${rec.id}`);
    // On a phone the record opens as a sheet, on a laptop as a side panel: both are `.record`.
    await page
      .locator(".record")
      .getByRole("button", { name: /^Finish/ })
      .click();
    const toast = page.locator(".toast").filter({ hasText: "Sent for approval." });
    await expect(toast).toBeVisible();
    const link = toast.getByRole("link", { name: "Open Approvals" });
    await expect(link).toHaveAttribute("href", /#\/approvals\?id=[0-9a-zA-Z-]+/);
    // Rule 10: the toast has no button that approves or rejects, and no plan id as text.
    await expect(toast.getByRole("button", { name: /approve|reject/i })).toHaveCount(0);
    await expect(toast).not.toContainText(/plan [0-9a-f]{8}/);
    expect((await api.run("ivy", `${MODEL}.get`, { id: rec.id })).state).toBe("doing");
    // Leave the instance as the test found it: ivy rejects the plan the toast links to, and her
    // waiting count is what it was before.
    const href = (await link.getAttribute("href")) ?? "";
    const plan = new URLSearchParams(href.slice(href.indexOf("?") + 1)).get("id");
    expect(plan, href).toBeTruthy();
    expect(await waiting(api, ivy)).toBe(before + 1);
    const rejected = await api.post("/api/plan/reject", { id: plan }, ivy);
    expect(rejected.status, JSON.stringify(rejected.json)).toBe(200);
    expect(await waiting(api, ivy)).toBe(before);
  });
});

test.describe("the approvals inbox on a laptop", () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test("the list is a table that fits its box and the detail is the side panel", async ({
    page,
    api,
    uid,
    scratchModule,
    signIn,
  }) => {
    await scratchModule(spec);
    const one = await propose(api, uid);
    await signIn("ivy", "/");
    await expect(
      page.getByRole("link", { name: /^Approvals, \d+ waiting for you$/ }),
    ).toBeVisible();
    await page.goto("/#/approvals");
    const table = page.getByRole("table");
    await expect(table).toBeVisible();
    const fit = await table.evaluate((el) => {
      const box = el.closest(".appr-page, .panel, .main") ?? el.parentElement!;
      return { table: el.getBoundingClientRect().width, box: box.getBoundingClientRect().width };
    });
    expect(fit.table).toBeLessThanOrEqual(fit.box + 1);
    await expect(page.getByRole("button", { name: /^(Approve|Reject)/ })).toHaveCount(0);
    expect(await layoutProblems(page), "the list at 1280 px").toEqual([]);

    await page.getByRole("button", { name: new RegExp(one.title) }).click();
    const panel = panelOf(page);
    await expect(panel).toBeVisible();
    // Beside the list, not over it.
    await expect(table).toBeVisible();
    await expect(panel.getByRole("button", { name: /^Approve and/ })).toBeVisible();
    expect(await layoutProblems(page), "the side panel at 1280 px").toEqual([]);
    // Leave nothing waiting that this test made.
    await panel.getByRole("button", { name: "Reject", exact: true }).click();
    await page
      .getByRole("alertdialog", { name: "Reject this request?" })
      .getByRole("button", { name: "Reject request" })
      .click();
    await expect(page.locator(".toast").filter({ hasText: "Rejected." })).toBeVisible();
  });
});

/**
 * What the list and the detail must not do with a title that has no space in it (a part number, an
 * e-mail address): push the page or the table's box sideways, break a short word in two, or put the
 * date of a row on two lines. Everything is measured in the browser, from boxes and from the lines of
 * a `Range`.
 */
async function listProblems(page: Page, opts: { oneLineDates: boolean }): Promise<string[]> {
  await page.evaluate(() =>
    Promise.all(document.getAnimations().map((a) => a.finished.catch(() => null))),
  );
  return page.evaluate((oneLineDates) => {
    const out: string[] = [];
    const de = document.documentElement;
    if (de.scrollWidth > de.clientWidth + 1)
      out.push(`the page scrolls sideways: ${de.scrollWidth} > ${de.clientWidth}`);
    for (const box of document.querySelectorAll<HTMLElement>(".table-wrap, aside[data-plan-panel]"))
      if (box.scrollWidth > box.clientWidth + 1)
        out.push(
          `${box.className || box.tagName} scrolls sideways: ${box.scrollWidth} > ${box.clientWidth}`,
        );
    const lines = (node: Node) => {
      const r = document.createRange();
      r.selectNodeContents(node);
      return [...r.getClientRects()].filter((q) => q.width > 0).length;
    };
    if (oneLineDates) {
      // The cells under the headers "Asked" and "Expires", found by their header, not by a class.
      const heads = [...document.querySelectorAll("thead th")].map((h) => h.textContent?.trim());
      const dates = [heads.indexOf("Asked"), heads.indexOf("Expires")];
      if (dates.includes(-1)) out.push("the list has no Asked and Expires columns");
      for (const tr of document.querySelectorAll("tbody tr"))
        for (const at of dates) {
          const td = tr.children[at];
          if (td && lines(td) !== 1) out.push(`"${td.textContent}" is on ${lines(td)} lines`);
        }
    }
    // A word of fewer than 20 characters is never broken inside (hyphens and dashes may break).
    const walker = document.createTreeWalker(
      document.querySelector(".appr-page") ?? document.body,
      NodeFilter.SHOW_TEXT,
    );
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = n.textContent ?? "";
      for (const m of text.matchAll(/[^\s\-–—]+/g)) {
        if (m[0].length >= 20) continue;
        const r = document.createRange();
        r.setStart(n, m.index!);
        r.setEnd(n, m.index! + m[0].length);
        const pieces = [...r.getClientRects()].filter((q) => q.width > 0).length;
        if (pieces > 1) out.push(`the word "${m[0]}" is broken in ${pieces} pieces`);
      }
    }
    return out;
  }, opts.oneLineDates);
}

/** 70 letters and digits with no space, unique to the test: a record title nobody can break by eye. */
const unbroken = (uid: (p: string) => string) =>
  (
    uid("part").replace(/[^a-z0-9]/gi, "") +
    "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz"
  ).slice(0, 70);

test.describe("the list and the detail with a title that has no space", () => {
  test("a table that fits its box with dates on one line, at 390, 768 and 1280 px", async ({
    page,
    api,
    uid,
    scratchModule,
    signIn,
  }) => {
    await scratchModule(spec);
    const ivy = await api.token("ivy");
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: [`${MODEL}.user`, `${MODEL}.manager`], label: "long title agent" },
      ivy,
    );
    const agent: string = issued.json.token;
    const plain = uid("Chairs");
    const long = unbroken(uid);
    expect(long).toHaveLength(70);
    expect(long).not.toMatch(/\s/);
    for (const title of [plain, long]) {
      const rec = await api.create("ivy", MODEL, { title, amount: 5 });
      await api.run("ivy", `${MODEL}.start`, { id: rec.id });
      await proposeFinish(api, agent, rec.id);
    }
    await page.setViewportSize({ width: 1280, height: 800 });
    await signIn("ivy", "/");

    // A phone: one block per row; the long title breaks inside its own line.
    for (const [w, h] of [
      [390, 844],
      [320, 640],
    ] as const) {
      await page.setViewportSize({ width: w, height: h });
      await page.goto("/#/approvals");
      await expect(page.getByRole("button", { name: new RegExp(long.slice(0, 20)) })).toBeVisible();
      expect(await listProblems(page, { oneLineDates: false }), `the list at ${w} px`).toEqual([]);
      expect(await layoutProblems(page), `the list at ${w} px`).toEqual([]);
    }

    // A tablet and a laptop: a table, and the dates never wrap.
    for (const [w, h] of [
      [768, 1024],
      [1280, 800],
    ] as const) {
      await page.setViewportSize({ width: w, height: h });
      await page.goto("/#/approvals");
      await expect(page.getByRole("table")).toBeVisible();
      expect(await listProblems(page, { oneLineDates: true }), `the list at ${w} px`).toEqual([]);
      expect(await layoutProblems(page), `the list at ${w} px`).toEqual([]);
      if (w === 1280) {
        // The Request column takes what the other three leave, not a fixed share of the table.
        const share = await page.evaluate(() => {
          const table = document.querySelector("table")!;
          const first = document.querySelector("tbody tr td")!;
          return first.getBoundingClientRect().width / table.getBoundingClientRect().width;
        });
        expect(share, "the Request column's share of the table at 1280 px").toBeGreaterThan(0.6);
      }
    }

    // The laptop with the detail open beside the list: the room is smaller, the same holds.
    await page.getByRole("button", { name: new RegExp(plain) }).click();
    await expect(panelOf(page)).toBeVisible();
    await expect(page.getByRole("table")).toBeVisible();
    expect(
      await listProblems(page, { oneLineDates: true }),
      "the list at 1280 px, panel open",
    ).toEqual([]);
    expect(await layoutProblems(page), "the list at 1280 px, panel open").toEqual([]);

    // Leave nothing waiting that this test made.
    for (const row of [plain, long]) {
      await page.goto("/#/approvals");
      await page.getByRole("button", { name: new RegExp(row.slice(0, 30)) }).click();
      await panelOf(page).getByRole("button", { name: "Reject", exact: true }).click();
      await page
        .getByRole("alertdialog", { name: "Reject this request?" })
        .getByRole("button", { name: "Reject request" })
        .click();
      await expect(page.locator(".toast").filter({ hasText: "Rejected." })).toBeVisible();
    }
  });

  test("the detail at 320 and 390 px: no sideways scroll, a heading in full, a footer button that wraps", async ({
    page,
    api,
    uid,
    scratchModule,
    signIn,
  }) => {
    await scratchModule({
      ...spec,
      actions: [
        ...spec.actions!,
        // The action's label is its name in words: this is the button a person reads.
        {
          name: "settle_the_claim_as_paid_and_close_it",
          field: "state",
          from: "new",
          to: "done",
          approval: true,
        },
      ],
    });
    const ivy = await api.token("ivy");
    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: [`${MODEL}.user`, `${MODEL}.manager`], label: "narrow agent" },
      ivy,
    );
    const agent: string = issued.json.token;
    const rec = await api.create("ivy", MODEL, { title: unbroken(uid), amount: 5 });
    await api.run("ivy", `${MODEL}.start`, { id: rec.id });
    const plan = await proposeFinish(api, agent, rec.id);
    const rec2 = await api.create("ivy", MODEL, { title: uid("Stool"), amount: 5 });
    const r = await api.post(
      `/api/${MODEL}/settle_the_claim_as_paid_and_close_it`,
      { id: rec2.id },
      agent,
    );
    expect(r.status, JSON.stringify(r.json)).toBe(202);
    const settle: string = r.json.plan_id;

    await page.setViewportSize({ width: 390, height: 700 });
    await signIn("ivy", "/");
    for (const width of [390, 320]) {
      await page.setViewportSize({ width, height: 700 });
      for (const id of [plan, settle]) {
        await page.goto(`/#/approvals?id=${id}`);
        const panel = dialogOf(page);
        await expect(panel).toBeVisible();
        await expect(panel.getByRole("button", { name: /^Approve and/ })).toBeVisible();
        const sizes = await panel.evaluate((el) => {
          const h = el.querySelector("h2")!;
          const b = [...el.querySelectorAll<HTMLElement>("footer .btn")];
          return {
            panelOver: el.scrollWidth - el.clientWidth,
            headingCut: h.scrollHeight - h.clientHeight,
            buttonsOver: b.map((x) => x.scrollWidth - x.clientWidth),
            buttonTexts: b.map((x) => x.textContent),
          };
        });
        expect(sizes.panelOver, `the panel at ${width} px scrolls sideways`).toBeLessThanOrEqual(0);
        expect(sizes.headingCut, "the heading is cut short").toBeLessThanOrEqual(1);
        expect(
          sizes.buttonsOver.every((o) => o <= 0),
          `a footer button overflows: ${sizes.buttonTexts}`,
        ).toBe(true);
        expect(await layoutProblems(page), `the detail at ${width} px`).toEqual([]);
      }
    }
    // The long action's label really is long, so the check above means something.
    await expect(
      dialogOf(page).getByRole("button", {
        name: "Approve and settle the claim as paid and close it",
      }),
    ).toBeVisible();

    // Leave nothing waiting that this test made.
    for (const id of [plan, settle]) {
      await page.goto(`/#/approvals?id=${id}`);
      await dialogOf(page).getByRole("button", { name: "Reject", exact: true }).click();
      await page
        .getByRole("alertdialog", { name: "Reject this request?" })
        .getByRole("button", { name: "Reject request" })
        .click();
      await expect(page.locator(".toast").filter({ hasText: "Rejected." })).toBeVisible();
    }
  });
});

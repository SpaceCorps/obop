// @vitest-environment jsdom
// The Approvals page (D97), mounted for real: the page, its plan panel, the data layer and a fake
// `fetch` that answers the five `/api/plan/*` commands as the server does (HANDOVER-server "What the
// page can call"). Each state of design 3.4 and of the hand-over has a test here, and rule 10 (no
// decision from a list) is held by tests that press everything the list offers.
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { ModuleMeta } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { href, nav, parse, type Go, type Route } from "../route.ts";
import { ApprovalsPage } from "./ApprovalsPage.tsx";
import type { PlanDetail, PlanRow } from "./planApi.ts";
import { plansChanged } from "./usePlanCount.ts";

vi.mock("./usePlanCount.ts", async (original) => ({
  ...(await original<typeof import("./usePlanCount.ts")>()),
  plansChanged: vi.fn(),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const HOUR = 3600_000;
const iso = (ms: number) => new Date(ms).toISOString().replace(/\.\d+Z$/, "Z");
const now = Date.now();

const modules: ModuleMeta[] = [
  {
    id: "invoicing",
    name: "Invoicing",
    models: [
      {
        name: "invoice",
        description: "",
        can_create: true,
        can_update: true,
        can_delete: true,
        fields: [
          {
            name: "amount",
            kind: "text",
            description: "",
            class: "none",
            personal_data: false,
            required: false,
          },
          {
            name: "state",
            kind: "select",
            options: ["draft", "posted"],
            description: "",
            class: "none",
            personal_data: false,
            required: false,
          },
          {
            name: "client",
            kind: "ref",
            to: "contact",
            description: "",
            class: "none",
            personal_data: false,
            required: false,
          },
          {
            name: "customer_note",
            kind: "text",
            description: "",
            class: "none",
            personal_data: false,
            required: false,
          },
        ],
        actions: [
          {
            id: "invoice.post",
            description: "",
            needs_approval: true,
            field: "state",
            from: "draft",
            to: "posted",
          },
        ],
      },
    ],
  },
];

function row(over: Partial<PlanRow> = {}): PlanRow {
  return {
    id: "p-update",
    command: "invoice.update",
    model: "invoice",
    action: "update",
    kind: "update",
    record_id: "inv-1",
    record_title: "INV-0042",
    proposer: "ada/agent",
    owner: "ada",
    own_agent: false,
    created_at: iso(now - 2 * HOUR),
    expires_at: iso(now + 5 * 24 * HOUR),
    status: "waiting",
    decided_by: null,
    decided_at: null,
    reason: null,
    failure: null,
    destructive: false,
    can_approve: true,
    can_reject: true,
    proposer_can_run: true,
    ...over,
  };
}
const SECRET = "s3cret-value-never-shown";
function detail(over: Partial<PlanDetail> = {}): PlanDetail {
  return {
    ...row(),
    changes: [
      { field: "amount", from: "1200.00", to: "1500.00", hidden: false },
      { field: "customer_note", from: null, to: "Paid in two parts", hidden: false },
      { field: "bank_secret", from: null, to: null, hidden: true },
    ],
    seen_version: 7,
    record: { id: "inv-1" } as never,
    confirm: null,
    done: null,
    edges: [],
    ...over,
  } as PlanDetail;
}

type Answer = { status: number; body: unknown };
const ok = (value: unknown): Answer => ({ status: 200, body: { value, denied: [] } });
const fail = (status: number, body: Record<string, unknown>): Answer => ({ status, body });

/** What the fake server holds; each command may be replaced by a test. */
let server: {
  waiting: PlanRow[];
  decided: PlanRow[];
  list: (state: string, body: Record<string, unknown>) => Answer;
  get: (id: string) => Answer;
  approve: (body: { id: string; seen_version?: number }) => Answer;
  reject: (body: { id: string; reason?: string }) => Answer;
};
let calls: { command: string; body: Record<string, unknown> }[];
/** A command whose answer waits until the test lets it go. */
let holds: Record<string, Promise<void>>;
const holdBack = (command: string): (() => void) => {
  let release: () => void = () => {};
  holds[command] = new Promise<void>((r) => (release = r));
  return release;
};
let plans: Map<string, PlanDetail>;
let notify: ReturnType<typeof vi.fn>;
let container: HTMLDivElement;
let root: Root;

function fakeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url).replace(/\?.*$/, "");
      const body = init?.body ? JSON.parse(String(init.body)) : {};
      const command = /^\/api\/plan\/([a-z]+)$/.exec(path)?.[1] ?? path;
      calls.push({ command, body });
      const hold = holds[command];
      if (hold) await hold;
      let a: Answer;
      if (command === "list") a = server.list(body.state, body);
      else if (command === "get") a = server.get(body.id);
      else if (command === "approve") a = server.approve(body);
      else if (command === "reject") a = server.reject(body);
      else if (command === "count") a = ok({ waiting: 1, mine_waiting: 0, can_decide: true });
      else a = fail(404, { error: "not found", code: "not_found" });
      if (a.status === 0) throw new TypeError("Failed to fetch");
      return new Response(JSON.stringify(a.body), {
        status: a.status,
        headers: { "content-type": "application/json" },
      });
    }),
  );
}

let go: Go;
function Harness({ start }: { start: string }) {
  const [route, setRoute] = useState<Route>(() => parse(start));
  go = (r) => setRoute(parse(href(r)));
  return (
    <DataProvider
      token="person-token"
      modules={modules}
      apps={[]}
      notify={notify as never}
      confirm={async () => true}
      linkFor={(model, opts) => `#/invoicing/${model}?id=${opts?.id ?? ""}`}
    >
      <ApprovalsPage route={route} go={go} />
      <output data-testid="route">{href(route)}</output>
    </DataProvider>
  );
}

const settle = async (n = 3) => {
  for (let i = 0; i < n; i++)
    await act(async () => {
      await new Promise((done) => setTimeout(done, 0));
    });
};
let mounts = 0;
/** Mount the page afresh at an address (a new key: nothing of the last mount is kept). */
async function mount(start = "#/approvals") {
  await act(async () => {
    root.render(<Harness key={++mounts} start={start} />);
  });
  await settle();
}
const flat = (el: Element | null | undefined) =>
  (el?.textContent ?? "").replace(/\s+/g, " ").trim();
const page = () => flat(document.body);
const buttons = (scope: ParentNode = document.body) => [
  ...scope.querySelectorAll<HTMLButtonElement>("button"),
];
const button = (name: string, scope: ParentNode = document.body) =>
  buttons(scope).find((b) => flat(b) === name || b.getAttribute("aria-label") === name);
const click = async (el: Element | null | undefined) => {
  expect(el, "the control to press").toBeTruthy();
  await act(async () => {
    (el as HTMLElement).click();
  });
  await settle();
};
const sent = (command: string) => calls.filter((c) => c.command === command);
const panel = () => document.querySelector<HTMLElement>("[data-plan-panel]");
const routeNow = () => flat(document.querySelector('[data-testid="route"]'));
const type = (el: HTMLTextAreaElement, value: string) =>
  act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
const decisions = () => calls.filter((c) => c.command === "approve" || c.command === "reject");

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  history.replaceState(null, "", "/");
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
  nav.ask = null;
  nav.dirty = null;
  notify = vi.fn();
  vi.mocked(plansChanged).mockClear();
  calls = [];
  holds = {};
  plans = new Map([
    ["p-update", detail()],
    [
      "p-delete",
      detail({
        ...row({
          id: "p-delete",
          command: "invoice.delete",
          action: "delete",
          kind: "delete",
          record_title: "INV-0007",
          destructive: true,
          created_at: iso(now - 3 * HOUR),
        }),
        changes: [],
        seen_version: 3,
      }),
    ],
  ]);
  server = {
    waiting: [
      row({
        id: "p-delete",
        command: "invoice.delete",
        action: "delete",
        kind: "delete",
        record_title: "INV-0007",
        destructive: true,
        created_at: iso(now - 3 * HOUR),
      }),
      row(),
    ],
    decided: [
      row({
        id: "p-old",
        status: "rejected",
        decided_by: "bo",
        decided_at: iso(now - HOUR),
        reason: "Wrong amount",
      }),
    ],
    list: (state) => {
      const rows = state === "decided" ? server.decided : server.waiting;
      return ok({ rows, total: rows.length });
    },
    get: (id) => {
      const p = plans.get(id);
      return p ? ok(p) : fail(404, { error: "not found", code: "not_found" });
    },
    approve: () => ok({ id: "inv-1" }),
    reject: (b) => ok({ ...row({ id: b.id }), status: "rejected", reason: b.reason ?? null }),
  };
  fakeServer();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the list", () => {
  it("is busy while it loads, then shows the waiting requests in the server's order (oldest first)", async () => {
    const release = holdBack("list");
    await act(async () => {
      root.render(<Harness start="#/approvals" />);
    });
    expect(document.querySelector('[role="tabpanel"]')?.getAttribute("aria-busy")).toBe("true");
    expect(document.querySelector(".skeleton")).not.toBeNull();
    await act(async () => release());
    await settle();
    expect(document.querySelector('[role="tabpanel"]')?.getAttribute("aria-busy")).toBeNull();
    const things = [...document.querySelectorAll(".appr-thing")].map(flat);
    expect(things).toEqual(["Delete invoice “INV-0007”", "Change invoice “INV-0042”"]);
    expect(flat(document.querySelector("h1"))).toBe("Approvals");
    expect(document.querySelectorAll("h1")).toHaveLength(1);
  });

  it("says honestly that nothing is waiting", async () => {
    server.waiting = [];
    await mount();
    expect(page()).toContain("Nothing is waiting for you.");
    expect(page()).toContain(
      "When an agent proposes a change that needs a person, it appears here.",
    );
    expect(document.querySelector("table")).toBeNull();
  });

  it("says when the list could not be loaded and tries again", async () => {
    let answer = fail(500, { error: "storage" });
    server.list = () => answer;
    await mount();
    expect(page()).toContain("The approvals could not be loaded.");
    answer = ok({ rows: server.waiting, total: 2 });
    await click(button("Try again"));
    expect(document.querySelectorAll(".appr-thing")).toHaveLength(2);
  });

  it("builds rows from data and the page's words: who asked, a chip for what cannot be undone", async () => {
    server.waiting = [row({ owner: "ada", own_agent: false }), row({ id: "p2", own_agent: true })];
    await mount();
    const text = flat(document.querySelector("tbody"));
    expect(text).toContain("ada's agent");
    expect(text).toContain("Your own agent");
    expect(text, "no plan id is printed").not.toMatch(/p-update|p2\b/);
    server.waiting = [row({ id: "p-delete", kind: "delete", destructive: true })];
    await mount();
    expect(flat(document.querySelector(".appr-chip"))).toBe("Cannot be undone");
  });

  it("shows no record line when the record id is null (the agent's id was no record)", async () => {
    server.waiting = [row({ record_id: null, record_title: null })];
    await mount();
    expect(flat(document.querySelector(".appr-thing"))).toBe("Change invoice");
  });

  it("lists the decided requests on their own tab, with the outcome", async () => {
    await mount();
    await click(button("Decided"));
    expect(sent("list").at(-1)?.body).toMatchObject({ state: "decided" });
    expect(routeNow()).toBe("#/approvals?view=decided");
    const text = flat(document.querySelector("tbody"));
    expect(text).toContain("Rejected");
    expect(text).toContain("bo");
  });
});

describe("rule 10: nothing is decided from the list", () => {
  it("has exactly one control per row, and it only opens the plan", async () => {
    await mount();
    for (const tr of document.querySelectorAll("tbody tr")) {
      const controls = tr.querySelectorAll("button, a, input, [role=button]");
      expect(controls).toHaveLength(1);
      expect(flat(controls[0])).not.toMatch(/approve|reject/i);
    }
  });

  it("pressing every control of the list never starts an approve or a reject", async () => {
    await mount();
    const list = document.querySelector("section.appr-page")!;
    for (const b of buttons(list)) {
      await click(b);
      // Close what opened, so the next control of the list is pressed from the list.
      if (panel()) await click(button("Close"));
    }
    for (const tab of ["Decided", "Waiting"]) {
      await click(button(tab));
      for (const b of buttons(document.querySelector("tbody") ?? document.body)) {
        await click(b);
        if (panel()) await click(button("Close"));
      }
    }
    expect(decisions(), "a decision sent from the list").toEqual([]);
    expect(button("Approve and save", list), "an Approve on the list").toBeUndefined();
    expect(button("Reject", list), "a Reject on the list").toBeUndefined();
  });

  it("puts Approve and Reject only in the panel, with one plan and its change on screen", async () => {
    await mount();
    await click(document.querySelector('[data-plan="p-update"]'));
    const p = panel()!;
    expect(p).not.toBeNull();
    expect(document.querySelectorAll("[data-plan-panel]")).toHaveLength(1);
    expect(flat(p)).toContain("What changes");
    expect(button("Approve and save", p)).toBeTruthy();
    expect(button("Reject", p)).toBeTruthy();
  });
});

describe("the detail", () => {
  it("opens from a deep link and shows what changes, hidden fields as set but not shown", async () => {
    await mount("#/approvals?id=p-update");
    const p = panel()!;
    expect(flat(p.querySelector("h2"))).toBe("An agent asks to change invoice “INV-0042”.");
    const pairs = [...p.querySelectorAll(".appr-change")].map(flat);
    expect(pairs).toContain("Amount1200.00 → 1500.00");
    expect(pairs).toContain("Customer noteNot set → Paid in two parts");
    expect(pairs).toContain("Bank secretSet, not shown to you");
    expect(flat(p), "a secret value on screen").not.toContain(SECRET);
    expect(flat(p)).toContain("Proposed by ada's agent");
    expect(flat(p)).toMatch(/Expires on /);
    expect(p.querySelector('a[href="#/invoicing/invoice?id=inv-1"]')?.textContent).toBe(
      "Open the record",
    );
  });

  it("answers an id that is not waiting (404) with the same sentence, not an error page", async () => {
    await mount("#/approvals?id=nope");
    expect(flat(panel())).toContain(
      "This request is not waiting any more. It may have been decided or it expired.",
    );
    expect(button("Reject", panel()!)).toBeUndefined();
    expect(document.querySelector("[data-testid=plan-approve]")).toBeNull();
  });

  it("names the person's own agent inside the sentence in lower case", async () => {
    plans.set("p-update", detail({ own_agent: true }));
    await mount("#/approvals?id=p-update");
    expect(flat(panel())).toContain("Proposed by your own agent,");
  });

  it("has no record line when record_id is null", async () => {
    plans.set("p-update", detail({ record_id: null, record_title: null }));
    await mount("#/approvals?id=p-update");
    expect(flat(panel()?.querySelector("h2"))).toBe("An agent asks to change invoice.");
    expect(panel()!.querySelector("a")).toBeNull();
  });

  it("shows a bulk create's first rows and a transition", async () => {
    plans.set(
      "p-bulk",
      detail({
        ...row({ id: "p-bulk", kind: "bulk_create", command: "invoice.bulk_create" }),
        changes: [],
        rows: [
          [{ field: "amount", from: null, to: "10.00", hidden: false }],
          [{ field: "amount", from: null, to: "20.00", hidden: false }],
        ],
        rows_total: 12,
      }),
    );
    await mount("#/approvals?id=p-bulk");
    expect(flat(panel())).toContain("12 records will be created. The first ones:");
    expect(flat(panel())).toContain("Amount10.00");
    expect(button("Approve and create", panel()!)).toBeTruthy();
    plans.set(
      "p-post",
      detail({
        ...row({ id: "p-post", kind: "action", command: "invoice.post", action: "post" }),
        changes: [],
        transition: { field: "state", from: "draft", to: "posted" },
      }),
    );
    await mount("#/approvals?id=p-post");
    expect(flat(panel()?.querySelector("h2"))).toBe("An agent asks to post invoice “INV-0042”.");
    expect(flat(panel()?.querySelector(".appr-change"))).toBe("StateDraft → Posted");
    expect(button("Approve and post", panel()!)).toBeTruthy();
  });

  it("shows a decided plan's outcome with no decision to make", async () => {
    plans.set(
      "p-old",
      detail({ id: "p-old", status: "approved", decided_by: "bo", decided_at: iso(now - HOUR) }),
    );
    await mount("#/approvals?id=p-old");
    expect(flat(panel())).toMatch(/bo approved this .*ago\./);
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
  });

  it("offers only Reject, and says why, when the agent's owner can no longer do this", async () => {
    plans.set(
      "p-update",
      detail({ can_approve: false, can_reject: true, proposer_can_run: false }),
    );
    await mount("#/approvals?id=p-update");
    expect(flat(panel())).toContain("The agent's owner can no longer do this");
    expect(document.querySelector("[data-testid=plan-approve]")).toBeNull();
    expect(button("Reject", panel()!)).toBeTruthy();
  });

  it("gives focus back to the row it was opened from when it closes", async () => {
    await mount();
    const opener = document.querySelector<HTMLElement>('[data-plan="p-update"]')!;
    opener.focus();
    await click(opener);
    expect(panel()).not.toBeNull();
    await click(button("Close"));
    await act(async () => {
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    });
    expect(panel()).toBeNull();
    expect(document.activeElement).toBe(document.querySelector('[data-plan="p-update"]'));
  });
});

describe("approve", () => {
  it("sends the version that was shown, says Approved and goes back to the list", async () => {
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("approve")).toEqual([
      { command: "approve", body: { id: "p-update", seen_version: 7 } },
    ]);
    expect(notify).toHaveBeenCalledWith("Approved.");
    expect(routeNow()).toBe("#/approvals");
    expect(panel()).toBeNull();
  });

  it("uses the action's own words after it ran (ui.done)", async () => {
    plans.set("p-update", detail({ done: "The invoice is posted." }));
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(notify).toHaveBeenCalledWith("Approved. The invoice is posted.");
  });

  it("asks again before a destructive plan, focus on Cancel; Cancel sends nothing", async () => {
    await mount("#/approvals?id=p-delete");
    expect(flat(panel())).toContain("This deletes “INV-0007” for good.");
    await click(button("Approve and delete"));
    const dialog = document.querySelector('[role="alertdialog"]')!;
    expect(flat(dialog)).toContain("Delete “INV-0007” for good? This cannot be undone.");
    expect(document.activeElement).toBe(button("Cancel", dialog));
    await click(button("Cancel", dialog));
    expect(sent("approve")).toEqual([]);
    await click(button("Approve and delete"));
    await click(button("Delete for good"));
    expect(sent("approve")).toEqual([
      { command: "approve", body: { id: "p-delete", seen_version: 3 } },
    ]);
  });

  it("asks with the action's own ui.confirm when it has one", async () => {
    plans.set(
      "p-delete",
      detail({
        ...plans.get("p-delete")!,
        confirm: "Remove this invoice and its lines?",
      }),
    );
    await mount("#/approvals?id=p-delete");
    await click(button("Approve and delete"));
    expect(flat(document.querySelector('[role="alertdialog"]'))).toContain(
      "Remove this invoice and its lines?",
    );
  });

  it("cannot be sent twice: a second press while busy sends nothing and the button is disabled", async () => {
    const release = holdBack("approve");
    await mount("#/approvals?id=p-update");
    const approve = document.querySelector<HTMLButtonElement>("[data-testid=plan-approve]")!;
    await act(async () => {
      approve.click();
      approve.click();
    });
    const busy = document.querySelector<HTMLButtonElement>("[data-testid=plan-approve]")!;
    expect(busy.disabled).toBe(true);
    expect(button("Reject", panel()!)?.disabled).toBe(true);
    await act(async () => busy.click());
    await act(async () => release());
    await settle();
    expect(sent("approve")).toHaveLength(1);
  });

  it("412: the record changed; the detail reloads, says so, takes focus, and Approve works again with the new version", async () => {
    let first = true;
    server.approve = (b) => {
      if (first) {
        first = false;
        plans.set(
          "p-update",
          detail({
            seen_version: 8,
            changes: [{ field: "amount", from: "1300.00", to: "1500.00", hidden: false }],
          }),
        );
        return fail(412, {
          error: "the record changed",
          code: "precondition_failed",
          current_version: 8,
        });
      }
      return ok({ id: b.id });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    const note = document.querySelector<HTMLElement>("[data-testid=plan-note]")!;
    expect(flat(note)).toBe("This record changed after you opened the request. Look at it again.");
    expect(note.getAttribute("role")).toBe("alert");
    expect(document.activeElement).toBe(note);
    expect(sent("get").length, "the detail was read again").toBeGreaterThanOrEqual(2);
    expect(flat(panel())).toContain("1300.00 → 1500.00");
    const again = document.querySelector<HTMLButtonElement>("[data-testid=plan-approve]")!;
    expect(again.disabled).toBe(false);
    await click(again);
    expect(sent("approve").map((c) => c.body)).toEqual([
      { id: "p-update", seen_version: 7 },
      { id: "p-update", seen_version: 8 },
    ]);
    expect(notify).toHaveBeenCalledWith("Approved.");
  });

  it("409 already_decided: says who decided and when, and offers no button", async () => {
    server.approve = () =>
      fail(409, {
        error: "this request was decided before",
        code: "already_decided",
        state: "rejected",
        decided_by: "bo",
        decided_at: iso(now - 2 * 60_000),
      });
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    const note = document.querySelector<HTMLElement>("[data-testid=plan-note]")!;
    expect(flat(note)).toBe("bo rejected this 2 minutes ago.");
    expect(document.activeElement).toBe(note);
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
  });

  it("409 already_decided because it expired: says when, and that nothing changed", async () => {
    server.approve = () =>
      fail(409, {
        error: "this request was decided before",
        code: "already_decided",
        state: "expired",
        decided_by: null,
        decided_at: null,
      });
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toMatch(
      /^This request expired on .+\. Nothing was changed\. Ask the agent to propose it again\.$/,
    );
  });

  it("404 while plan.get still returns the plan: reloads it and does not say it is not waiting", async () => {
    server.approve = () => {
      plans.set("p-update", detail({ can_approve: false, proposer_can_run: false }));
      return fail(404, { error: "not found", code: "not_found" });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(flat(panel())).not.toContain("not waiting any more");
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toBe(
      "This request could not be approved as it stands. Look at it again.",
    );
    expect(flat(panel())).toContain("The agent's owner can no longer do this");
    expect(document.querySelector("[data-testid=plan-approve]")).toBeNull();
  });

  it("404 when the plan is gone: the same not-waiting sentence as for any hidden plan", async () => {
    server.approve = () => {
      plans.delete("p-update");
      return fail(404, { error: "not found", code: "not_found" });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(flat(panel())).toContain(
      "This request is not waiting any more. It may have been decided or it expired.",
    );
    expect(document.querySelector("[data-testid=plan-approve]")).toBeNull();
  });

  it("400 at approval that spent the plan: the system refused it, the request is closed", async () => {
    server.approve = () => {
      plans.set("p-update", detail({ status: "failed", failure: "bad_request", decided_by: "me" }));
      return fail(400, { error: "amount must be positive", code: "bad_request" });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    const note = document.querySelector<HTMLElement>("[data-testid=plan-note]")!;
    expect(flat(note)).toBe(
      "The system refused it: amount must be positive. This request is closed. The agent can propose it again.",
    );
    expect(document.activeElement).toBe(note);
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
  });

  it("400 from a guard: the request is still waiting and can be decided", async () => {
    server.approve = () => fail(400, { error: "the record moved on", code: "bad_request" });
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toBe(
      "The system refused it: the record moved on. The request is still waiting.",
    );
    expect(document.querySelector("[data-testid=plan-approve]")).not.toBeNull();
  });

  it("503 or a lost connection: asks to check before trying again, and reads the plan again", async () => {
    server.approve = () => {
      plans.set(
        "p-update",
        detail({ status: "approved", decided_by: "me", decided_at: iso(now - 1000) }),
      );
      return fail(503, {
        error: "storage unavailable",
        code: "storage_unavailable",
        applied: "unknown",
      });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toBe(
      "We could not reach the server. Check whether it went through before trying again.",
    );
    expect(flat(panel())).toMatch(/me approved this /);
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
    // A connection that broke: the same, and a decision is never sent again by itself.
    server.approve = () => ({ status: 0, body: null });
    plans.set("p-update", detail());
    await mount("#/approvals?id=p-update");
    const before = sent("approve").length;
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("approve").length - before).toBe(1);
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toBe(
      "We could not reach the server. Check whether it went through before trying again.",
    );
  });
});

describe("reject", () => {
  const reasonField = () =>
    document.querySelector<HTMLTextAreaElement>('textarea[name="plan_reject_reason"]')!;

  it("asks once with an optional reason, sends it in its own field, and says the agent will see it", async () => {
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    const dialog = document.querySelector('[role="alertdialog"]')!;
    expect(flat(dialog)).toContain("Reject this request?");
    expect(flat(dialog)).toContain("Tell the agent why (optional)");
    await type(reasonField(), "  Wrong customer  ");
    await click(button("Reject request"));
    expect(sent("reject")).toEqual([
      { command: "reject", body: { id: "p-update", reason: "Wrong customer" } },
    ]);
    expect(notify).toHaveBeenCalledWith("Rejected. The agent will see your reason.");
    expect(routeNow()).toBe("#/approvals");
  });

  it("sends no reason when none was given", async () => {
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    await click(button("Reject request"));
    expect(sent("reject")[0]?.body).toEqual({ id: "p-update" });
  });

  it("counts the reason: 300 characters go, 301 do not", async () => {
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    await type(reasonField(), "é".repeat(300));
    expect(page()).toContain("300 of 300 characters");
    expect(button("Reject request")?.disabled).toBe(false);
    await type(reasonField(), "é".repeat(301));
    expect(page()).toContain("301 of 300 characters: shorten it by 1.");
    expect(button("Reject request")?.disabled).toBe(true);
    await click(button("Reject request"));
    expect(sent("reject")).toEqual([]);
    // Characters, as the server counts them: an emoji is one, not two UTF-16 units.
    await type(reasonField(), "😀".repeat(300));
    expect(button("Reject request")?.disabled).toBe(false);
  });

  it("409 on reject: who decided first, and no button", async () => {
    server.reject = () =>
      fail(409, {
        error: "this request was decided before",
        code: "already_decided",
        state: "approved",
        decided_by: "bo",
        decided_at: iso(now - 60_000),
      });
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    await click(button("Reject request"));
    expect(flat(document.querySelector("[data-testid=plan-note]"))).toBe(
      "bo approved this 1 minute ago.",
    );
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
  });
});

describe("after the review", () => {
  const reasonField = () =>
    document.querySelector<HTMLTextAreaElement>('textarea[name="plan_reject_reason"]')!;
  const bulk = (over: Partial<PlanDetail>) =>
    plans.set(
      "p-bulk",
      detail({
        ...row({ id: "p-bulk", kind: "bulk_create", command: "invoice.bulk_create" }),
        changes: [],
        ...over,
      }),
    );

  it("a bulk create proposed as csv shows no rows, says so, and offers only Reject", async () => {
    bulk({ rows: null, rows_total: 12 } as never);
    await mount("#/approvals?id=p-bulk");
    expect(flat(panel())).not.toContain("The first ones");
    expect(flat(panel())).toContain(
      "12 records will be created, but their rows are not shown here.",
    );
    expect(flat(panel())).toContain("this cannot be approved from this page");
    expect(button("Approve and create", panel()!)).toBeUndefined();
    expect(button("Reject", panel()!)).toBeTruthy();
    expect(decisions()).toEqual([]);
  });

  it("a bulk row that is not a record does not break the panel; a person can still reject", async () => {
    bulk({
      rows: [null, [{ field: "amount", from: null, to: "10.00", hidden: false }]],
      rows_total: 2,
    } as never);
    await mount("#/approvals?id=p-bulk");
    expect(flat(panel())).toContain("This row is not shown.");
    expect(flat(panel())).toContain("Amount10.00");
    expect(button("Approve and create", panel()!)).toBeTruthy();
    bulk({ rows: [null], rows_total: 1 } as never);
    await mount("#/approvals?id=p-bulk");
    expect(button("Reject", panel()!)).toBeTruthy();
    expect(button("Approve and create", panel()!)).toBeUndefined();
  });

  it("a waiting plan nobody here can decide says it waits for somebody else", async () => {
    plans.set("p-update", detail({ own_agent: true, can_approve: false, can_reject: false }));
    await mount("#/approvals?id=p-update");
    expect(flat(panel())).toContain("Waiting for a person who can approve it.");
    expect(panel()!.querySelector(".appr-foot")).toBeNull();
  });

  it("reads more than fifty: says how many are shown and fetches the next ones", async () => {
    const all = Array.from({ length: 70 }, (_, i) => row({ id: `p${i}`, record_title: `R${i}` }));
    server.list = (_s, body) => {
      const at = Number(body.offset ?? 0);
      return ok({ rows: all.slice(at, at + 50), total: 70 });
    };
    await mount();
    expect(document.querySelectorAll(".appr-thing")).toHaveLength(50);
    expect(page()).toContain("Showing 50 of 70.");
    await click(button("Show more"));
    expect(sent("list").at(-1)?.body).toMatchObject({ offset: 50 });
    expect(document.querySelectorAll(".appr-thing")).toHaveLength(70);
    expect(page()).not.toContain("Show more");
  });

  it("reads the list again after a decision, and tells the count", async () => {
    await mount("#/approvals?id=p-update");
    const before = sent("list").length;
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("list").length).toBeGreaterThan(before);
    expect(plansChanged).toHaveBeenCalled();
    vi.mocked(plansChanged).mockClear();
    await mount("#/approvals?id=p-update");
    const beforeReject = sent("list").length;
    await click(button("Reject", panel()!));
    await click(button("Reject request"));
    expect(sent("list").length).toBeGreaterThan(beforeReject);
    expect(plansChanged).toHaveBeenCalled();
  });

  it("a 404 or a 400 from the decision reads the list and the count again", async () => {
    server.approve = () => fail(400, { error: "the record moved on", code: "bad_request" });
    await mount("#/approvals?id=p-update");
    let before = sent("list").length;
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("list").length).toBeGreaterThan(before);
    expect(plansChanged).toHaveBeenCalled();
    vi.mocked(plansChanged).mockClear();
    server.approve = () => fail(404, { error: "not found", code: "not_found" });
    before = sent("list").length;
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("list").length).toBeGreaterThan(before);
    expect(plansChanged).toHaveBeenCalled();
  });

  it("does not close while a decision is on its way (Escape, Close)", async () => {
    const release = holdBack("approve");
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    await act(async () => {
      dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    await click(button("Close"));
    expect(panel(), "closed with a decision in flight").not.toBeNull();
    await act(async () => release());
    await settle();
    expect(panel()).toBeNull();
    expect(notify).toHaveBeenCalledWith("Approved.");
    // Once nothing is in flight, Escape closes it again.
    await mount("#/approvals?id=p-update");
    await act(async () => {
      dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(panel()).toBeNull();
  });

  it("a decided plan shows its outcome and no change block that reads as still to come", async () => {
    plans.set(
      "p-gone",
      detail({
        ...plans.get("p-delete")!,
        id: "p-gone",
        status: "rejected",
        decided_by: "bo",
        decided_at: iso(now - HOUR),
        can_approve: false,
        can_reject: false,
      }),
    );
    await mount("#/approvals?id=p-gone");
    expect(flat(panel())).toMatch(/bo rejected this /);
    expect(flat(panel())).not.toContain("What changes");
    expect(flat(panel())).not.toContain("for good");
    plans.set(
      "p-ex",
      detail({ status: "expired", changes: undefined, can_approve: false, can_reject: false }),
    );
    await mount("#/approvals?id=p-ex");
    expect(flat(panel())).not.toContain("No changed field that you may read.");
  });

  it("412 and the plan is gone after the reload: says it is not waiting, not that the record changed", async () => {
    server.approve = () => {
      plans.delete("p-update");
      return fail(412, { error: "changed", code: "precondition_failed", current_version: 9 });
    };
    await mount("#/approvals?id=p-update");
    await click(document.querySelector("[data-testid=plan-approve]"));
    const note = flat(document.querySelector("[data-testid=plan-note]"));
    expect(note).toContain("not waiting any more");
    expect(note).not.toContain("This record changed");
  });

  it("a reference in the change shows no short id while its records are not known", async () => {
    plans.set(
      "p-update",
      detail({
        changes: [
          {
            field: "client",
            from: "3f9a2c1d-0000-4000-8000-000000000001",
            to: "8b1e44a0-0000-4000-8000-000000000002",
            hidden: false,
          },
        ],
      }),
    );
    await mount("#/approvals?id=p-update");
    await settle(10);
    const text = flat(panel()!.querySelector(".appr-change"));
    expect(text).toContain("Not shown to you");
    expect(text).not.toMatch(/3f9a2c1d|8b1e44a0/);
  });

  it("counts a reason as the server does: hidden characters do not count", async () => {
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    await type(reasonField(), "é".repeat(300) + "​⁠");
    expect(page()).toContain("300 of 300 characters");
    expect(button("Reject request")?.disabled).toBe(false);
    await type(reasonField(), "é".repeat(301) + "​");
    expect(button("Reject request")?.disabled).toBe(true);
  });

  it("opening a deep link decides nothing", async () => {
    await mount("#/approvals?id=p-update");
    expect(decisions()).toEqual([]);
  });

  it("a deep link whose plan cannot be read says so, and is not the not-waiting sentence", async () => {
    server.get = () => fail(500, { error: "storage" });
    await mount("#/approvals?id=p-update");
    expect(flat(panel())).toContain("This request could not be loaded.");
    expect(flat(panel())).not.toContain("not waiting any more");
    expect(button("Try again", panel()!)).toBeTruthy();
  });

  it("offers Approve alone when the person may approve but not reject", async () => {
    plans.set("p-update", detail({ can_approve: true, can_reject: false }));
    await mount("#/approvals?id=p-update");
    expect(button("Approve and save", panel()!)).toBeTruthy();
    expect(button("Reject", panel()!)).toBeUndefined();
  });

  it("a delete has no link to the record that is about to go", async () => {
    await mount("#/approvals?id=p-delete");
    expect(panel()!.querySelector("a.appr-record")).toBeNull();
    await mount("#/approvals?id=p-update");
    expect(panel()!.querySelector("a.appr-record")).not.toBeNull();
  });

  it("sends the version that was shown, not one read at the click", async () => {
    let gets = 0;
    server.get = (id) => {
      gets++;
      const p = plans.get(id)!;
      return ok({ ...p, seen_version: gets === 1 ? 7 : 99 });
    };
    await mount("#/approvals?id=p-update");
    const readsBefore = sent("get").length;
    await click(document.querySelector("[data-testid=plan-approve]"));
    expect(sent("get").length, "no read at the click").toBe(readsBefore);
    expect(sent("approve")[0]?.body).toEqual({ id: "p-update", seen_version: 7 });
  });

  it("rejects once when the button is pressed twice", async () => {
    const release = holdBack("reject");
    await mount("#/approvals?id=p-update");
    await click(button("Reject", panel()!));
    const confirm = button("Reject request")!;
    await act(async () => {
      confirm.click();
      confirm.click();
    });
    await act(async () => release());
    await settle();
    expect(sent("reject")).toHaveLength(1);
  });
});

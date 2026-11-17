// @vitest-environment jsdom
// The agent tokens section of Customize > Security, mounted for real: the real section, a fake
// `fetch` that answers the three account routes like the server, and jsdom for the DOM. The tests
// drive it the way a person does and read the requests, the page and the browser's state afterwards.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { fmt, provideCatalogue, setLanguage } from "@obop/ui";
import { AgentTokensSection } from "./AgentTokens.tsx";
import type { Notify } from "./SecurityAccount.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SECRET = `obop_${"s".repeat(43)}`;
const ID = "a1".repeat(32);
const CREATED = 1731600000;

type TokenRow = {
  id: string;
  label: string;
  scopes: string[];
  created_at: number;
  expires_at?: number | null;
};
type Call = { method: string; path: string; auth: string; body: unknown };

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn<Notify>>;
let clipboard: { writeText: ReturnType<typeof vi.fn> };
let calls: Call[];
/** What the fake server holds and how it answers. */
let server: {
  tokens: TokenRow[];
  delegable: string[];
  /** The status of the list; 200 answers with the tokens. */
  list: number;
  /** A sentence makes `create` answer 400 with it. */
  refuse: string | null;
};

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });

function fakeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url);
      const method = init?.method ?? "GET";
      const auth = (init?.headers as Record<string, string> | undefined)?.authorization ?? "";
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ method, path, auth, body });
      if (method === "GET" && path === "/api/auth/agent-tokens")
        return server.list === 200
          ? json({ tokens: server.tokens, delegable: server.delegable })
          : json({ error: "refused" }, server.list);
      if (method === "POST" && path === "/api/auth/agent-token") {
        if (server.refuse) return json({ error: server.refuse }, 400);
        const made = body as { label: string; scopes: string[] };
        server.tokens = [
          ...server.tokens,
          { id: "b2".repeat(32), label: made.label, scopes: made.scopes, created_at: CREATED },
        ];
        return json({ token: SECRET });
      }
      const revoke = /^\/api\/auth\/agent-tokens\/([^/]+)\/revoke$/.exec(path);
      if (method === "POST" && revoke) {
        const before = server.tokens.length;
        server.tokens = server.tokens.filter((row) => row.id !== revoke[1]);
        return server.tokens.length < before ? json({ revoked: true }) : json({}, 404);
      }
      return json({}, 404);
    }),
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
async function mount() {
  await act(async () => {
    root.render(<AgentTokensSection token="session-token" notify={notify} />);
  });
  await settle();
}
const text = () => container.textContent ?? "";
const button = (name: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => (b.textContent ?? "").trim() === name || b.getAttribute("aria-label") === name,
  );
const click = async (el: Element | null | undefined) => {
  await act(async () => {
    (el as HTMLElement).click();
  });
  await settle();
};
const type = (input: HTMLInputElement, value: string) =>
  act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
const labelField = () =>
  container.querySelector<HTMLInputElement>('input[name="agent_token_label"]');
const tokenField = () => container.querySelector<HTMLInputElement>('[data-testid="agent-token"]');
const groups = () =>
  [...container.querySelectorAll<HTMLLabelElement>(".group-list label")].map(
    (l) => l.textContent ?? "",
  );
const group = (name: string) =>
  [...container.querySelectorAll<HTMLLabelElement>(".group-list label")]
    .find((l) => l.textContent === name)!
    .querySelector<HTMLInputElement>("input")!;
const sent = (method: string, path: string) =>
  calls.filter((c) => c.method === method && c.path === path);

/** Fill the form in and press Create. */
async function create(label = "  CI bot ", picks = ["crm.user", "base.user"]) {
  await type(labelField()!, label);
  for (const g of picks) await click(group(g));
  await click(button("Create agent token"));
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  sessionStorage.clear();
  history.replaceState(null, "", "/");
  notify = vi.fn<Notify>();
  clipboard = { writeText: vi.fn(async () => {}) };
  Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
  calls = [];
  server = {
    tokens: [
      { id: ID, label: "Reporting", scopes: ["base.user", "crm.user"], created_at: CREATED },
    ],
    delegable: ["base.user", "crm.user", "documents.user"],
    list: 200,
    refuse: null,
  };
  fakeServer();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the list of agent tokens", () => {
  it("shows when a token ends by itself, and Until revoked for one that does not (N18)", async () => {
    const ENDS = CREATED + 90 * 86_400;
    server.tokens = [
      { ...server.tokens[0], expires_at: ENDS },
      { id: "c3".repeat(32), label: "Old", scopes: ["base.user"], created_at: CREATED },
    ];
    await mount();
    const cells = [...container.querySelectorAll('tbody td[data-label="Ends"]')].map(
      (td) => td.textContent,
    );
    expect(cells).toEqual([fmt.dateTime(ENDS * 1000), "Until revoked"]);
  });

  it("shows each token with its label, its groups and when it was made", async () => {
    await mount();
    const row = container.querySelector("tbody tr")!;
    expect(row.textContent).toContain("Reporting");
    expect(row.textContent).toContain("base.user, crm.user");
    expect(row.textContent).toContain(fmt.dateTime(CREATED * 1000));
    expect(container.querySelector("caption")?.textContent).toBe("Agent tokens");
    expect(sent("GET", "/api/auth/agent-tokens")).toHaveLength(1);
    expect(calls[0]!.auth).toBe("Bearer session-token");
  });

  it("says so when there are none", async () => {
    server.tokens = [];
    await mount();
    expect(text()).toContain("No agent tokens.");
    expect(container.querySelector("table")).toBeNull();
  });

  it("shows a refused read as one, not as an empty list", async () => {
    server.list = 500;
    await mount();
    expect(text()).toContain("The agent tokens could not be loaded. Try again.");
    expect(text()).not.toContain("No agent tokens.");
    expect(labelField()).toBeNull();
    server.list = 200;
    await click(button("Try again"));
    expect(text()).toContain("Reporting");
    expect(text()).not.toContain("could not be loaded");
  });

  it("shows nothing where the server has no such route", async () => {
    server.list = 404;
    await mount();
    expect(container.innerHTML).toBe("");
  });

  it("revokes a token by its id, says so and reads the list again", async () => {
    await mount();
    await click(button("Revoke Reporting"));
    const revoked = sent("POST", `/api/auth/agent-tokens/${ID}/revoke`);
    expect(revoked).toHaveLength(1);
    expect(notify).toHaveBeenCalledWith("Agent token Reporting revoked.");
    expect(sent("GET", "/api/auth/agent-tokens")).toHaveLength(2);
    expect(text()).toContain("No agent tokens.");
  });

  it("says when a token could not be revoked", async () => {
    await mount();
    server.tokens = []; // it went away in another window
    await click(button("Revoke Reporting"));
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0]![1]).toBe("error");
    expect(text()).toContain("No agent tokens.");
  });
});

describe("creating an agent token", () => {
  it("offers the groups the person may hand over, none ticked, and a filter only above twelve", async () => {
    await mount();
    expect(groups()).toEqual(["base.user", "crm.user", "documents.user"]);
    expect(container.querySelectorAll(".group-list input:checked")).toHaveLength(0);
    expect(text()).not.toContain("Filter groups");
    await act(async () => root.unmount());
    root = createRoot(container);
    server.delegable = Array.from({ length: 13 }, (_, i) => `app${i}.user`);
    await mount();
    expect(text()).toContain("Filter groups");
  });

  it("ties the label to its field and limits it to 80 characters", async () => {
    await mount();
    const field = labelField()!;
    expect(container.querySelector(`label[for="${field.id}"]`)?.textContent).toContain("Label");
    expect(field.maxLength).toBe(80);
    expect(field.required).toBe(true);
  });

  it("keeps Create disabled until there is a label and a group", async () => {
    await mount();
    expect(button("Create agent token")!.disabled).toBe(true);
    await type(labelField()!, "CI bot");
    expect(button("Create agent token")!.disabled).toBe(true);
    await click(group("crm.user"));
    expect(button("Create agent token")!.disabled).toBe(false);
    await type(labelField()!, "   ");
    expect(button("Create agent token")!.disabled).toBe(true);
  });

  it("sends the label and the groups, shows the token once and reads the list again", async () => {
    await mount();
    await create();
    const made = sent("POST", "/api/auth/agent-token");
    expect(made).toHaveLength(1);
    expect(made[0]!.body).toEqual({ scopes: ["base.user", "crm.user"], label: "CI bot" });
    expect(tokenField()!.value).toBe(SECRET);
    expect(tokenField()!.readOnly).toBe(true);
    expect(container.querySelector(`label[for="${tokenField()!.id}"]`)).not.toBeNull();
    expect(text()).toContain("This is the only time the token is shown. Store it like a password.");
    expect(document.activeElement).toBe(button("Copy"));
    expect(sent("GET", "/api/auth/agent-tokens")).toHaveLength(2);
    expect(container.querySelectorAll("tbody tr")).toHaveLength(2);
    // Another token cannot be made over the one that is still on the screen.
    expect(labelField()).toBeNull();
  });

  it("takes the token off the page with Done and gives an empty form back", async () => {
    await mount();
    await create();
    await click(button("Done"));
    expect(tokenField()).toBeNull();
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(labelField()!.value).toBe("");
    expect(container.querySelectorAll(".group-list input:checked")).toHaveLength(0);
    expect(document.activeElement).toBe(labelField());
  });

  it("copies the token and says so, without the token in the notification", async () => {
    await mount();
    await create();
    await click(button("Copy"));
    expect(clipboard.writeText).toHaveBeenCalledTimes(1);
    expect(clipboard.writeText).toHaveBeenCalledWith(SECRET);
    expect(notify.mock.calls).toEqual([["Copied"]]);
  });

  it("says when the browser refused the copy", async () => {
    clipboard.writeText.mockRejectedValue(new Error("denied"));
    await mount();
    await create();
    await click(button("Copy"));
    expect(notify.mock.calls).toEqual([
      ["Could not copy. Select the token and copy it yourself.", "error"],
    ]);
    // The token stays on the page for the person to copy by hand.
    expect(tokenField()!.value).toBe(SECRET);
  });

  it("holds the token nowhere but on the screen", async () => {
    const before = { href: location.href, history: history.length, title: document.title };
    const logged: unknown[][] = [];
    for (const level of ["log", "info", "warn", "error", "debug"] as const)
      vi.spyOn(console, level).mockImplementation((...args) => void logged.push(args));
    await mount();
    await create();
    await click(button("Copy"));
    const hold = (v: unknown) => JSON.stringify(v) ?? "";
    for (const when of ["shown", "put away"]) {
      expect(localStorage.length, `localStorage, ${when}`).toBe(0);
      expect(sessionStorage.length, `sessionStorage, ${when}`).toBe(0);
      expect(document.cookie, `cookie, ${when}`).toBe("");
      expect(location.href, `the address, ${when}`).toBe(before.href);
      expect(history.length, `history, ${when}`).toBe(before.history);
      expect(hold(history.state), `history.state, ${when}`).not.toContain(SECRET);
      expect(document.title, `the title, ${when}`).toBe(before.title);
      expect(hold(notify.mock.calls), `a notification, ${when}`).not.toContain(SECRET);
      expect(hold(logged), `the console, ${when}`).not.toContain(SECRET);
      if (when === "shown") await click(button("Done"));
    }
    // No request carried it back either.
    for (const c of calls) expect(`${c.path} ${hold(c.body)}`).not.toContain(SECRET);
  });

  it("shows the server's sentence when it refuses, and keeps what was typed", async () => {
    server.refuse = "the label must not contain line breaks";
    await mount();
    await create("CI bot", ["crm.user"]);
    expect(container.querySelector('[role="alert"]')?.textContent).toBe(
      "the label must not contain line breaks",
    );
    expect(tokenField()).toBeNull();
    expect(labelField()!.value).toBe("CI bot");
    expect(group("crm.user").checked).toBe(true);
    expect(notify).not.toHaveBeenCalled();
  });

  it("says so when the person has no group to hand over", async () => {
    server.delegable = [];
    await mount();
    expect(text()).toContain("You have no permission groups to give to a token.");
    expect(labelField()).toBeNull();
  });
});

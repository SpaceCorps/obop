// @vitest-environment jsdom
// The ways into the Approvals page (stage P2): the top-bar button with its count, the account
// menu's item, the polite announcement of a change, and the toast "Sent for approval." whose link
// only opens the page (rule 10: nothing decides from the bar or a toast).
import { act, type ComponentProps } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { TopBar } from "../Chrome.tsx";
import { ApprovalsButton, entryName } from "./ApprovalsEntry.tsx";
import type { PlanCount } from "./planApi.ts";
import { notifySent, sentText } from "./sent.ts";
import { plansChanged } from "./usePlanCount.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let count: PlanCount | null;
let hits: string[];

const none = () => {};
const bar = (token = "person-token") => {
  const props: ComponentProps<typeof TopBar> = {
    apps: [],
    current: undefined,
    email: "ann@example.test",
    menuOpen: false,
    hasMenu: false,
    onMenu: none,
    onLauncher: none,
    onPalette: none,
    onSignOut: none,
    onTheme: none,
    onBrand: none,
    onSecurity: none,
    token,
  };
  return <TopBar {...props} />;
};
const settle = () => act(async () => new Promise((r) => setTimeout(r, 0)));
const entry = () => container.querySelector<HTMLAnchorElement>("a.appr-entry");

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  count = { waiting: 3, mine_waiting: 0, can_decide: true };
  hits = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      hits.push(String(url));
      if (count === null) return new Response("{}", { status: 404 });
      return new Response(JSON.stringify({ value: count }), {
        headers: { "content-type": "application/json" },
      });
    }),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  location.hash = "";
});

describe("the top-bar button", () => {
  it("shows the count, names it in words and only links to the page", async () => {
    await act(async () => root.render(bar()));
    await settle();
    const a = entry();
    expect(a).not.toBeNull();
    expect(a!.getAttribute("href")).toBe("#/approvals");
    expect(a!.getAttribute("aria-label")).toBe("Approvals, 3 waiting for you");
    expect(a!.textContent).toBe("3");
    expect(container.querySelector("header button[aria-label*='Approve' i]")).toBeNull();
  });

  it("shows 9+ above nine, and the name of an empty inbox of a person who can decide", async () => {
    count = { waiting: 12, mine_waiting: 0, can_decide: true };
    await act(async () => root.render(bar()));
    await settle();
    expect(entry()!.textContent).toBe("9+");
    expect(entry()!.getAttribute("aria-label")).toBe("Approvals, 12 waiting for you");
    expect(entryName({ waiting: 0, mine_waiting: 0, can_decide: true })).toBe(
      "Approvals, nothing waiting",
    );
  });

  it("is shown when the person can decide with nothing waiting, and not when they cannot", async () => {
    count = { waiting: 0, mine_waiting: 0, can_decide: true };
    await act(async () => root.render(bar()));
    await settle();
    expect(entry()).not.toBeNull();
    expect(entry()!.textContent).toBe("");
    expect(entry()!.getAttribute("aria-label")).toBe("Approvals, nothing waiting");
    await act(async () => root.unmount());
    root = createRoot(container);
    count = { waiting: 0, mine_waiting: 0, can_decide: false };
    await act(async () => root.render(bar()));
    await settle();
    expect(entry()).toBeNull();
  });

  it("is not shown, and asks nothing, without a token or when the server has no count", async () => {
    await act(async () => root.render(bar("")));
    await settle();
    expect(entry()).toBeNull();
    expect(hits).toEqual([]);
    await act(async () => root.unmount());
    root = createRoot(container);
    count = null;
    await act(async () => root.render(bar()));
    await settle();
    expect(entry()).toBeNull();
  });

  it("announces a change of the count politely, once, and not the first count", async () => {
    await act(async () => root.render(bar()));
    await settle();
    const live = () => container.querySelector('[role="status"][aria-live="polite"]');
    expect(live()!.textContent).toBe("");
    count = { waiting: 4, mine_waiting: 0, can_decide: true };
    await act(async () => plansChanged());
    await settle();
    expect(live()!.textContent).toBe("Approvals, 4 waiting for you");
    expect(container.querySelectorAll('[role="status"][aria-live="polite"]').length).toBe(1);
  });
});

describe("the account menu", () => {
  const items = async () => {
    const trigger = container.querySelector<HTMLButtonElement>("button.avatar-btn")!;
    await act(async () => trigger.click());
    return [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
  };

  it("has an Approvals item with the count, that opens the page", async () => {
    await act(async () => root.render(bar()));
    await settle();
    const found = (await items()).find((i) => i.textContent?.startsWith("Approvals"));
    expect(found).toBeDefined();
    expect(found!.textContent).toContain("3");
    await act(async () => found!.click());
    expect(location.hash).toBe("#/approvals");
  });

  it("has no Approvals item for a person who sees nothing of it", async () => {
    count = { waiting: 0, mine_waiting: 0, can_decide: false };
    await act(async () => root.render(bar()));
    await settle();
    expect((await items()).some((i) => i.textContent?.startsWith("Approvals"))).toBe(false);
  });
});

describe("the toast of a plan sent for approval", () => {
  it("says one sentence with a link to that plan and never the plan's id as text", () => {
    const notify = vi.fn();
    notifySent(notify, "01HZX9PLANIDXYZ");
    expect(notify).toHaveBeenCalledTimes(1);
    const [text, tone, opts] = notify.mock.calls[0]!;
    expect(text).toBe("Sent for approval.");
    expect(sentText()).toBe(text);
    expect(tone).toBe("info");
    expect(opts.action.href).toBe("#/approvals?id=01HZX9PLANIDXYZ");
    expect(opts.action.label).toBe("Open Approvals");
    expect(opts.action.onClick).toBeUndefined();
    expect(text).not.toMatch(/01HZX9PL/);
  });
});

describe("the button on its own", () => {
  it("gives a screen reader the count in the name, not in the badge", async () => {
    await act(async () =>
      root.render(<ApprovalsButton count={{ waiting: 2, mine_waiting: 0, can_decide: true }} />),
    );
    expect(entry()!.getAttribute("aria-label")).toBe("Approvals, 2 waiting for you");
    expect(entry()!.querySelector(".appr-badge")!.getAttribute("aria-hidden")).toBe("true");
  });
});

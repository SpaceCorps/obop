// @vitest-environment jsdom
// K25 (d): a read of the metadata and the settings that began before a sign-out and finishes
// after it belongs to the session that ended: it must not put that session's theme or screens
// back on the sign-in screen.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { App } from "./App.tsx";

// Customize stands in for any screen that asks the shell to read the metadata again (a module
// was installed or removed).
vi.mock("./Customize.tsx", () => ({
  Customize: (props: { onModulesChanged: () => void }) => (
    <button type="button" className="reload-modules" onClick={() => props.onModulesChanged()}>
      reload
    </button>
  ),
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
/** While set, the reads of the metadata and the settings wait for it. */
let hold: Promise<void> | null = null;
let release: () => void = () => {};

beforeEach(() => {
  history.replaceState(null, "", "/#/customize/theme?tab=theme");
  sessionStorage.setItem("obop-token", "tok");
  sessionStorage.setItem("obop-user", "ann@example.test");
  hold = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const path = new URL(url, "http://localhost").pathname;
      const reply = (status: number, json: unknown) =>
        new Response(JSON.stringify(json), { status });
      if ((path === "/api/meta" || path === "/api/setting/get") && hold) await hold;
      if (path === "/api/meta") return reply(200, { modules: [] });
      if (path === "/api/setting/get") {
        return reply(200, {
          value: { value: JSON.stringify({ theme: { mode: "dark", density: "compact" } }) },
        });
      }
      if (path === "/api/instance_brand/get") return reply(200, { value: { set: false } });
      return reply(404, { error: `no ${path}` });
    }),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  sessionStorage.clear();
  vi.unstubAllGlobals();
  const html = document.documentElement;
  delete html.dataset.theme;
  delete html.dataset.density;
  delete html.dataset.radius;
  html.removeAttribute("style");
});

async function settle() {
  for (let i = 0; i < 8; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

describe("signing out while the workspace is being read again", () => {
  it("does not put the old session's theme or workspace back when the read finishes", async () => {
    await act(async () => root.render(<App />));
    await settle();
    const html = document.documentElement;
    expect(html.dataset.theme).toBe("dark");
    const reload = container.querySelector<HTMLButtonElement>("button.reload-modules");
    expect(reload).not.toBeNull();

    // The next reads wait; the person asks for a reload and then signs out.
    hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    await act(async () => reload!.click());
    const trigger = container.querySelector<HTMLButtonElement>("button.avatar-btn");
    expect(trigger).not.toBeNull();
    await act(async () => trigger!.click());
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
      (el) => el.textContent === "Sign out",
    );
    expect(item).toBeDefined();
    await act(async () => item!.click());
    await settle();
    expect(html.dataset.theme).toBeUndefined();

    // The stale reads now finish.
    release();
    await settle();
    expect(sessionStorage.getItem("obop-token")).toBeNull();
    expect(html.dataset.theme).toBeUndefined();
    expect(html.dataset.density).toBeUndefined();
    expect(container.querySelector("button.avatar-btn")).toBeNull();
    expect(container.querySelector('input[type="password"]')).not.toBeNull();
  });
});

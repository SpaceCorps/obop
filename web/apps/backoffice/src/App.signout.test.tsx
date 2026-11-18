// @vitest-environment jsdom
// K25 (e): what the instance's settings put on the page (its theme) goes with the session, so the
// sign-in screen after an in-app sign-out looks as one after a reload.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { App } from "./App.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  history.replaceState(null, "", "/");
  sessionStorage.setItem("obop-token", "tok");
  sessionStorage.setItem("obop-user", "ann@example.test");
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const path = new URL(url, "http://localhost").pathname;
      const reply = (status: number, json: unknown) =>
        new Response(JSON.stringify(json), { status });
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
  for (let i = 0; i < 6; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

describe("signing out", () => {
  it("takes the instance theme off the page", async () => {
    await act(async () => root.render(<App />));
    await settle();
    const html = document.documentElement;
    expect(html.dataset.theme).toBe("dark");
    expect(html.dataset.density).toBe("compact");
    const trigger = container.querySelector<HTMLButtonElement>("button.avatar-btn");
    expect(trigger).not.toBeNull();
    await act(async () => trigger!.click());
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
      (el) => el.textContent === "Sign out",
    );
    expect(item).toBeDefined();
    await act(async () => item!.click());
    await settle();
    expect(sessionStorage.getItem("obop-token")).toBeNull();
    expect(html.dataset.theme).toBeUndefined();
    expect(html.dataset.density).toBeUndefined();
    expect(html.style.getPropertyValue("--accent")).toBe("");
  });
});

// @vitest-environment jsdom
// How `App` wires the switch that hides the Brand book and the Guide (docs/spec/branding.md,
// section 6 and 14). The page reads it with `instance_brand.get` together with the metadata after
// sign-in, before the first screen; a person who signs out leaves the instance it belonged to, so
// the next sign-in starts from "shown"; and a read that fails changes nothing, so a page that had
// the documents hidden keeps them hidden and one that had them shown keeps them shown. The
// predicate itself and the seven places that ask it are held by `productDocs.test.ts`,
// `Chrome.docs.test.tsx` and `Identity.test.tsx`.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { App } from "./App.tsx";
import { productDocsVisible, setProductDocsHidden } from "./productDocs.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
// What `instance_brand.get` answers: the switch on, off, or a refusal (the read fails).
let read: "on" | "off" | "fails";
let asked: string[];

beforeEach(() => {
  read = "off";
  asked = [];
  history.replaceState(null, "", "/");
  sessionStorage.setItem("obop-token", "tok");
  sessionStorage.setItem("obop-user", "ann@example.test");
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const path = new URL(url, "http://localhost").pathname;
      asked.push(path);
      const reply = (status: number, json: unknown) =>
        new Response(JSON.stringify(json), { status });
      if (path === "/api/meta") return reply(200, { modules: [] });
      if (path === "/api/setting/get") return reply(200, { value: { value: "{}" } });
      if (path === "/api/instance_brand/get") {
        return read === "fails"
          ? reply(403, { error: "not allowed" })
          : reply(200, { value: { set: true, name: "MOHV", hide_product_docs: read === "on" } });
      }
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
  setProductDocsHidden(false);
  sessionStorage.clear();
  vi.unstubAllGlobals();
});

/** Lets the metadata, the settings and the switch be read and the first screen be drawn. */
async function settle() {
  for (let i = 0; i < 6; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

async function open() {
  await act(async () => root.render(<App />));
  await settle();
}

async function signOut() {
  const trigger = container.querySelector<HTMLButtonElement>("button.avatar-btn");
  expect(trigger).not.toBeNull();
  await act(async () => trigger!.click());
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
    (el) => el.textContent === "Sign out",
  );
  expect(item).toBeDefined();
  await act(async () => item!.click());
  await settle();
}

describe("the switch in the app", () => {
  it("is read with the metadata, before the first screen, and hides the documents when it is on", async () => {
    read = "on";
    await open();
    expect(asked).toContain("/api/instance_brand/get");
    expect(productDocsVisible()).toBe(false);
  });

  it("leaves the documents shown when it is off", async () => {
    await open();
    expect(asked).toContain("/api/instance_brand/get");
    expect(productDocsVisible()).toBe(true);
  });

  it("is put back to shown when the person signs out, so the next sign-in starts from there", async () => {
    read = "on";
    await open();
    expect(productDocsVisible()).toBe(false);
    await signOut();
    expect(sessionStorage.getItem("obop-token")).toBeNull();
    expect(productDocsVisible()).toBe(true);
  });

  it("sends an address of the Brand book or the Guide to Theme, and the address says so, while they are hidden", async () => {
    for (const tab of ["guide", "brand"]) {
      read = "on";
      history.replaceState(null, "", `/#/customize/${tab}?tab=${tab}`);
      const entries = history.length;
      await open();
      expect(productDocsVisible(), tab).toBe(false);
      expect(location.hash, tab).toBe("#/customize/theme?tab=theme");
      // The address is replaced and no entry is added: Back does not return to the hidden address,
      // which would send the person to Theme again.
      expect(history.length, tab).toBe(entries);
      act(() => root.unmount());
      root = createRoot(container);
      setProductDocsHidden(false);
    }
  });

  it("leaves that address as it is while they are shown", async () => {
    history.replaceState(null, "", "/#/customize/guide?tab=guide");
    await open();
    expect(productDocsVisible()).toBe(true);
    expect(location.hash).toBe("#/customize/guide?tab=guide");
  });

  it("is not changed by a read that fails: shown stays shown, hidden stays hidden", async () => {
    read = "fails";
    await open();
    expect(asked).toContain("/api/instance_brand/get");
    expect(productDocsVisible()).toBe(true);
    // The page had read the switch as on before (a refresh of the metadata asks again).
    act(() => root.unmount());
    root = createRoot(container);
    setProductDocsHidden(true);
    await open();
    expect(productDocsVisible()).toBe(false);
  });
});

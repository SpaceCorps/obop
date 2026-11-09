// @vitest-environment jsdom
// The Identity tab of Customize, mounted against a stand-in for the three commands it may call
// (docs/spec/branding.md, sections 3, 6 and 13): it reads with `instance_brand.get`, writes with
// `configure` and `clear`, sends only what changed, shows the server's refusal, sets the switch
// that hides the Brand book and the Guide, and carries the notice that the text is public.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { IdentityTab } from "./Identity.tsx";
import { DataProvider } from "./data.tsx";
import { productDocsVisible, setProductDocsHidden } from "./productDocs.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let calls: { command: string; body: Record<string, unknown> }[];
let store: Record<string, unknown>;
let refuse: string | null;
let confirmed: boolean;
const notify = vi.fn();

const row = (extra: Record<string, unknown> = {}) => ({
  set: true,
  name: "MOHV",
  slug: "",
  v: "0123456789abcdef",
  hide_product_docs: false,
  message_en: "Welcome",
  effective: JSON.stringify({
    name: { value: "MOHV", source: "record" },
    slug: { value: "mohv", source: "record" },
    logo: { value: "tile", source: "record" },
  }),
  fallbacks: "[]",
  ...extra,
});
const none = () => ({
  set: false,
  name: "",
  v: "",
  hide_product_docs: false,
  effective: JSON.stringify({ logo: { value: "none", source: "default" } }),
  fallbacks: "[]",
});

beforeEach(() => {
  calls = [];
  store = row();
  refuse = null;
  confirmed = true;
  notify.mockClear();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit) => {
      const m = /^\/api\/([a-z_]+)\/([a-z_]+)/.exec(url);
      const command = `${m?.[1]}.${m?.[2]}`;
      const body = JSON.parse(String(init.body ?? "{}"));
      calls.push({ command, body });
      const reply = (status: number, json: unknown) =>
        new Response(JSON.stringify(json), { status });
      if (command === "instance_brand.get") return reply(200, { value: store, denied: [] });
      if (command === "instance_brand.configure") {
        if (refuse) return reply(400, { error: refuse });
        store = { ...store, set: true, ...body, v: store.v || "aaaaaaaaaaaaaaaa" };
        return reply(200, { value: { changes: "[]", v: store.v }, denied: [] });
      }
      if (command === "instance_brand.clear") {
        store = none();
        return reply(200, { value: { cleared: true }, denied: [] });
      }
      return reply(404, { error: `no ${command}` });
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
  vi.unstubAllGlobals();
});

async function mount() {
  await act(async () =>
    root.render(
      <DataProvider
        token="tok"
        modules={[]}
        apps={[]}
        notify={notify as never}
        confirm={async () => confirmed}
        linkFor={() => null}
      >
        <IdentityTab token="tok" notify={notify as never} />
      </DataProvider>,
    ),
  );
  await act(async () => {});
}

const field = (label: string) =>
  [...container.querySelectorAll<HTMLInputElement>("input")].find((i) =>
    i.labels?.[0]?.textContent?.startsWith(label),
  )!;
const button = (name: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name)!;

async function type(el: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const click = (el: HTMLElement) => act(async () => el.click());

describe("the Identity tab", () => {
  it("reads the record with get and fills the form", async () => {
    await mount();
    expect(calls.map((c) => c.command)).toEqual(["instance_brand.get"]);
    expect(field("Name").value).toBe("MOHV");
    expect(field("English").value).toBe("Welcome");
    expect(container.textContent).toContain("now: mohv");
  });

  it("tells the administrator that the text is readable by anyone", async () => {
    await mount();
    expect(container.textContent).toContain("Shown before anyone signs in");
    expect(container.textContent).toContain("can be read by anyone who opens the sign-in page");
  });

  it("keeps Save off until something changed, then sends only what changed", async () => {
    await mount();
    expect(button("Save").disabled).toBe(true);
    const messages = [...container.querySelectorAll<HTMLInputElement>("input")].filter(
      (i) => i.lang === "sv",
    );
    // The Swedish message input comes before the Swedish title input.
    await type(messages[0]!, "Välkommen");
    expect(button("Save").disabled).toBe(false);
    await click(button("Save"));
    const configure = calls.filter((c) => c.command === "instance_brand.configure");
    expect(configure).toHaveLength(1);
    expect(configure[0]!.body).toEqual({ message_sv: "Välkommen" });
    expect(notify).toHaveBeenCalled();
    // And the form was read again.
    expect(calls.at(-1)!.command).toBe("instance_brand.get");
    expect(button("Save").disabled).toBe(true);
  });

  it("calls the three commands and no other", async () => {
    await mount();
    await type(field("Name"), "MOHV AB");
    await click(button("Save"));
    await click(button("Remove the identity"));
    const used = new Set(calls.map((c) => c.command));
    expect([...used].sort()).toEqual([
      "instance_brand.clear",
      "instance_brand.configure",
      "instance_brand.get",
    ]);
  });

  it("sets the switch through configure and the app hides the documents at once", async () => {
    await mount();
    const sw = container.querySelector<HTMLInputElement>('input[role="switch"]')!;
    expect(sw.checked).toBe(false);
    expect(productDocsVisible()).toBe(true);
    await click(sw);
    await click(button("Save"));
    expect(calls.find((c) => c.command === "instance_brand.configure")!.body).toEqual({
      hide_product_docs: true,
    });
    expect(productDocsVisible()).toBe(false);
    expect(container.querySelector<HTMLInputElement>('input[role="switch"]')!.checked).toBe(true);
  });

  it("shows the refusal of the server and keeps what was typed", async () => {
    refuse = "name: must not be empty\nslug: starts with a digit";
    await mount();
    await type(field("Name"), "X");
    await click(button("Save"));
    const alert = container.querySelector('[role="alert"]')!;
    expect(alert.textContent).toContain("The identity was not saved");
    expect(alert.textContent).toContain("slug: starts with a digit");
    expect(field("Name").value).toBe("X");
    expect(notify).not.toHaveBeenCalled();
  });

  it("asks before it removes the identity, and does nothing when the answer is no", async () => {
    confirmed = false;
    await mount();
    await click(button("Remove the identity"));
    expect(calls.map((c) => c.command)).toEqual(["instance_brand.get"]);
    confirmed = true;
    await click(button("Remove the identity"));
    expect(calls.map((c) => c.command)).toEqual([
      "instance_brand.get",
      "instance_brand.clear",
      "instance_brand.get",
    ]);
    expect(productDocsVisible()).toBe(true);
    expect(container.querySelector("button.danger")).toBeNull(); // nothing left to remove
  });

  it("needs a name the first time, and offers no removal before there is anything", async () => {
    store = none();
    await mount();
    expect(button("Remove the identity")).toBeUndefined();
    const sw = container.querySelector<HTMLInputElement>('input[role="switch"]')!;
    await click(sw);
    expect(button("Save").disabled).toBe(true);
    expect(container.textContent).toContain("A name is needed the first time you save.");
    await type(field("Name"), "Acme");
    expect(button("Save").disabled).toBe(false);
  });

  it("previews the sign-in card from the form, with the message of the chosen language", async () => {
    await mount();
    await type(field("Name"), "Acme");
    const card = container.querySelector(".identity-card")!;
    expect(card.textContent).toContain("Acme");
    expect(card.textContent).toContain("Welcome");
    expect(card.querySelector("img")!.getAttribute("src")).toBe(
      "/api/brand/logo.svg?v=0123456789abcdef",
    );
    const select = container.querySelector<HTMLSelectElement>(".identity-preview select")!;
    await act(async () => {
      select.value = "sv";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    // Swedish has no message of its own: the English one stands in, as on the sign-in screen.
    expect(container.querySelector(".identity-card")!.textContent).toContain("Welcome");
  });

  it("says so when the record cannot be read", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("{}", { status: 500 })),
    );
    await mount();
    expect(container.textContent).toContain("The identity of this instance could not be read");
  });

  it("lists the stored values that cannot be used", async () => {
    store = row({ fallbacks: JSON.stringify([{ field: "title_sv", reason: "unsafe_character" }]) });
    await mount();
    expect(container.textContent).toContain("The default is shown instead of: title_sv.");
  });
});

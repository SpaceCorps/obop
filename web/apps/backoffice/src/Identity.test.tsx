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
// While set, the answer to `configure` waits for it: a save that is still running.
let hold: Promise<void> | null;
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
  hold = null;
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
        if (hold) await hold;
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
// A click with a pointer behind it has a detail of 1 or more; `click()` is one from the keyboard,
// which has none.
const pointer = (el: HTMLElement) =>
  act(async () => {
    el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 }));
  });

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

  it("switches the documents back on when the identity is removed", async () => {
    store = row({ hide_product_docs: true });
    await mount();
    setProductDocsHidden(true); // the page had read the switch at sign-in
    expect(container.querySelector<HTMLInputElement>('input[role="switch"]')!.checked).toBe(true);
    await click(button("Remove the identity"));
    expect(calls.map((c) => c.command)).toContain("instance_brand.clear");
    expect(productDocsVisible()).toBe(true);
  });

  it("keeps what is typed while a save is still running, and replaces the form when it is not", async () => {
    await mount();
    await type(field("Name"), "MOHV AB");
    let release!: () => void;
    hold = new Promise<void>((resolve) => (release = resolve));
    await click(button("Save")); // waits for the answer
    expect(button("Save").disabled).toBe(true);
    await type(field("English"), "Welcome back");
    release();
    await act(async () => {});
    // The name was saved and is the record's; what was typed since is still in the form and still
    // to be saved.
    expect(calls.find((c) => c.command === "instance_brand.configure")!.body).toEqual({
      name: "MOHV AB",
    });
    expect(field("Name").value).toBe("MOHV AB");
    expect(field("English").value).toBe("Welcome back");
    expect(button("Save").disabled).toBe(false);
    await click(button("Save"));
    expect(calls.filter((c) => c.command === "instance_brand.configure").at(-1)!.body).toEqual({
      message_en: "Welcome back",
    });
    expect(button("Save").disabled).toBe(true);
  });

  it("refuses a file that is too large or not an SVG, previews a good one and lets it go again", async () => {
    const created: string[] = [];
    const revoked: string[] = [];
    vi.stubGlobal("URL", {
      createObjectURL: () => {
        created.push(`blob:logo-${created.length}`);
        return created.at(-1)!;
      },
      revokeObjectURL: (u: string) => void revoked.push(u),
    });
    await mount();
    const input = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    const choose = (file: File) =>
      act(async () => {
        Object.defineProperty(input, "files", { value: [file], configurable: true });
        input.dispatchEvent(new Event("change", { bubbles: true }));
      });
    await choose(new File(["x".repeat(16 * 1024 + 1)], "big.svg", { type: "image/svg+xml" }));
    expect(container.querySelector('[role="alert"]')!.textContent).toContain(
      "The file is larger than 16 KiB",
    );
    await choose(new File(["x"], "logo.png", { type: "image/png" }));
    expect(container.querySelector('[role="alert"]')!.textContent).toContain("Choose an SVG file.");
    expect(button("Save").disabled).toBe(true);
    // A file of exactly 16 KiB is accepted: the limit is on what is more.
    await choose(new File(["x".repeat(16 * 1024)], "edge.SVG", { type: "" }));
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(container.textContent).toContain("edge.SVG will replace the logo when you save.");
    expect(button("Save").disabled).toBe(false);
    expect(container.querySelector(".identity-card img")!.getAttribute("src")).toBe(created[0]);
    await click(button("Undo"));
    expect(revoked).toEqual([created[0]]);
    expect(container.querySelector(".identity-card img")!.getAttribute("src")).toBe(
      "/api/brand/logo.svg?v=0123456789abcdef",
    );
  });

  it("draws a blank card before the first save, with no logo of the product's own", async () => {
    store = none();
    await mount();
    const src = container.querySelector(".identity-card img")!.getAttribute("src")!;
    expect(src).toMatch(/^data:image\/svg\+xml/);
    expect(src).not.toContain("/api/brand/logo.svg");
  });

  it("keeps a choice about the logo that is made while a save is still running", async () => {
    store = row({
      effective: JSON.stringify({
        name: { value: "MOHV", source: "record" },
        slug: { value: "mohv", source: "record" },
        logo: { value: "artwork", source: "record" },
      }),
    });
    await mount();
    await type(field("Name"), "MOHV AB");
    let release!: () => void;
    hold = new Promise<void>((resolve) => (release = resolve));
    await click(button("Save"));
    await click(button("Remove the logo"));
    release();
    await act(async () => {});
    expect(container.textContent).toContain("The logo will be removed when you save.");
    expect(button("Save").disabled).toBe(false);
    await click(button("Save"));
    expect(calls.filter((c) => c.command === "instance_brand.configure").at(-1)!.body).toEqual({
      logo_svg: "",
    });
  });

  it("saves on Enter in a field, as Save does, and only when there is something to save", async () => {
    await mount();
    const form = container.querySelector("form")!;
    const submit = () => act(async () => form.requestSubmit());
    await submit(); // nothing changed
    expect(calls.map((c) => c.command)).toEqual(["instance_brand.get"]);
    await type(field("Name"), "MOHV AB");
    await submit();
    expect(calls.filter((c) => c.command === "instance_brand.configure")).toHaveLength(1);
    // A first save with no name is not sent either.
    store = none();
    await act(async () => root.unmount());
    root = createRoot(container);
    calls = [];
    await mount();
    await click(container.querySelector<HTMLInputElement>('input[role="switch"]')!);
    await act(async () => container.querySelector("form")!.requestSubmit());
    expect(calls.map((c) => c.command)).toEqual(["instance_brand.get"]);
  });

  it("puts the language inputs in groups named by their heading and hides the file input", async () => {
    await mount();
    const groups = [...container.querySelectorAll<HTMLElement>('[role="group"][aria-labelledby]')];
    const named = (name: string) =>
      groups.find(
        (g) => document.getElementById(g.getAttribute("aria-labelledby")!)?.textContent === name,
      );
    for (const name of ["Message on the sign-in screen", "Titles on the pages of connected apps"]) {
      const g = named(name);
      expect(g, name).toBeDefined();
      expect(g!.querySelectorAll("input")).toHaveLength(7);
    }
    const file = container.querySelector<HTMLInputElement>('input[type="file"]')!;
    expect(file.getAttribute("aria-hidden")).toBe("true");
    expect(file.tabIndex).toBe(-1);
  });

  it("lets a switch that is stored as something else than a boolean be put right by saving", async () => {
    store = row({
      fallbacks: JSON.stringify([{ field: "hide_product_docs", reason: "not_boolean" }]),
    });
    await mount();
    expect(container.textContent).toContain("The default is shown instead of: hide_product_docs.");
    expect(button("Save").disabled).toBe(false);
    await click(button("Save"));
    expect(calls.find((c) => c.command === "instance_brand.configure")!.body).toEqual({
      hide_product_docs: false,
    });
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
    // The tab and the top bar keep the removed name until the page loads again: the toast says so,
    // as the one of a save does.
    expect(notify).toHaveBeenCalledWith(
      "Identity removed. The tab title and the logo in the top bar change when the page loads again.",
    );
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

  it("marks the name as required until there is a record, and not after", async () => {
    store = none();
    await mount();
    expect(field("Name").required).toBe(true);
    await act(async () => root.unmount());
    root = createRoot(container);
    store = row();
    await mount();
    expect(field("Name").required).toBe(false);
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
    // Swedish has no message of its own: the English one stands in, as on the sign-in screen, and
    // is marked as English for a screen reader.
    expect(container.querySelector(".identity-card")!.textContent).toContain("Welcome");
    const message = [...container.querySelectorAll(".identity-card p.hint")].at(-1)!;
    expect(message.textContent).toBe("Welcome");
    expect(message.getAttribute("lang")).toBe("en");
    await act(async () => {
      select.value = "en";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(
      [...container.querySelectorAll(".identity-card p.hint")].at(-1)!.hasAttribute("lang"),
    ).toBe(false);
  });

  it("cuts a text at the limit in characters as the server counts them, not in UTF-16 units", async () => {
    await mount();
    // 25 letters outside the Basic Multilingual Plane are 25 characters and 50 units: all kept.
    const han = (n: number) =>
      Array.from({ length: n }, (_, i) => String.fromCodePoint(0x20000 + i)).join("");
    await type(field("Name"), han(25));
    expect(field("Name").value).toBe(han(25));
    // 45 are 5 more than the 40 the name may have; 40 are kept, whole.
    await type(field("Name"), han(45));
    expect(Array.from(field("Name").value)).toEqual(Array.from(han(40)));
    expect(field("Name").hasAttribute("maxlength")).toBe(false);
    // The same for a message (240) and a title (60) and the short name (30).
    await type(field("English"), "x".repeat(250));
    expect(field("English").value.length).toBe(240);
    await type(field("Short name"), "a".repeat(31));
    expect(field("Short name").value.length).toBe(30);
    const titles = [...container.querySelectorAll<HTMLInputElement>("input")].filter(
      (i) => i.lang === "de",
    );
    await type(titles[1]!, han(61));
    expect(Array.from(titles[1]!.value).length).toBe(60);
  });

  it("previews a logo that is to be removed as no logo, and an undo as the saved one", async () => {
    store = row({
      effective: JSON.stringify({
        name: { value: "MOHV", source: "record" },
        slug: { value: "mohv", source: "record" },
        logo: { value: "artwork", source: "record" },
      }),
    });
    await mount();
    const saved = "/api/brand/logo.svg?v=0123456789abcdef";
    expect(container.querySelector(".identity-card img")!.getAttribute("src")).toBe(saved);
    expect(container.querySelector(".identity-tile img")!.getAttribute("src")).toBe(saved);
    await click(button("Remove the logo"));
    expect(container.textContent).toContain("The logo will be removed when you save.");
    // What saving would give is not the saved artwork: neither the card nor the tile draws it.
    expect(container.querySelector(".identity-card img")!.getAttribute("src")).toMatch(
      /^data:image\/svg\+xml/,
    );
    expect(container.querySelector(".identity-tile img")).toBeNull();
    expect(container.querySelector(".identity-tile-empty")).not.toBeNull();
    await click(button("Undo"));
    expect(container.querySelector(".identity-card img")!.getAttribute("src")).toBe(saved);
    expect(container.querySelector(".identity-tile img")!.getAttribute("src")).toBe(saved);
  });

  it("offers to remove the logo only when there is one, and Discard puts a choice about it back", async () => {
    await mount();
    // The stored logo is the tile of the name: there is no artwork to remove.
    expect(button("Remove the logo")).toBeUndefined();
    await act(async () => root.unmount());
    root = createRoot(container);
    store = row({
      effective: JSON.stringify({
        name: { value: "MOHV", source: "record" },
        slug: { value: "mohv", source: "record" },
        logo: { value: "artwork", source: "record" },
      }),
    });
    await mount();
    await type(field("Name"), "MOHV AB");
    await click(button("Remove the logo"));
    expect(container.textContent).toContain("The logo will be removed when you save.");
    await click(button("Discard changes"));
    expect(container.textContent).not.toContain("The logo will be removed when you save.");
    expect(button("Undo")).toBeUndefined();
    expect(field("Name").value).toBe("MOHV");
    expect(button("Save").disabled).toBe(true);
  });

  it("draws no landmark of its own: the groups are named by a heading and no field repeats it", async () => {
    await mount();
    expect(container.querySelectorAll('section, [role="region"]')).toHaveLength(0);
    // The only groups named by a heading are the two of language inputs, which a screen reader
    // reads out with the heading above them (the preview card has a label of its own, and so has
    // the hidden file input, but neither is a landmark); the Name field has one label and nothing
    // else called Name.
    const named = [...container.querySelectorAll("[aria-labelledby]")];
    expect(named.map((e) => e.getAttribute("role"))).toEqual(["group", "group"]);
    const names = [...container.querySelectorAll("h3, label")].filter(
      (e) => e.textContent === "Name",
    );
    expect(names.map((e) => e.tagName)).toEqual(["LABEL"]);
  });

  it("puts the focus on the first field after a save, because Save turns itself off", async () => {
    await mount();
    await type(field("English"), "Welcome back");
    button("Save").focus();
    expect(document.activeElement).toBe(button("Save"));
    await click(button("Save"));
    expect(button("Save").disabled).toBe(true);
    expect(document.activeElement).toBe(field("Name"));
  });

  it("puts the focus on the first field when nothing has it any more at the end of the save", async () => {
    await mount();
    await type(field("English"), "Welcome back");
    let release!: () => void;
    hold = new Promise<void>((resolve) => (release = resolve));
    button("Save").focus();
    await click(button("Save"));
    // The focus is on the page and nowhere else: a browser that resets it when the button turns off
    // (focus fixup), or a click on the page beside the form. A field that is focused and left
    // is the way to get there here, because jsdom cannot blur a button that is off.
    field("English").focus();
    field("English").blur();
    expect(document.activeElement).toBe(document.body);
    release();
    await act(async () => {});
    expect(document.activeElement).toBe(field("Name"));
  });

  it("puts the focus on the first field after Discard changes, which turns itself off", async () => {
    await mount();
    await type(field("English"), "Welcome back");
    button("Discard changes").focus();
    await click(button("Discard changes"));
    expect(button("Discard changes").disabled).toBe(true);
    expect(field("English").value).toBe("Welcome");
    expect(document.activeElement).toBe(field("Name"));
  });

  it("puts the focus on the first field after the identity is removed, because that button goes", async () => {
    await mount();
    button("Remove the identity").focus();
    await click(button("Remove the identity"));
    await act(async () => {});
    expect(calls.map((c) => c.command)).toContain("instance_brand.clear");
    expect(button("Remove the identity")).toBeUndefined();
    expect(document.activeElement).toBe(field("Name"));
  });

  it("leaves the focus alone after a save or a discard with the mouse, which a jump to the first field would scroll away from", async () => {
    await mount();
    await type(field("English"), "Welcome back");
    button("Discard changes").focus();
    await pointer(button("Discard changes"));
    expect(button("Discard changes").disabled).toBe(true);
    expect(document.activeElement).not.toBe(field("Name"));
    await type(field("English"), "Welcome back");
    button("Save").focus();
    await pointer(button("Save"));
    await act(async () => {});
    expect(calls.filter((c) => c.command === "instance_brand.configure")).toHaveLength(1);
    expect(button("Save").disabled).toBe(true);
    expect(document.activeElement).not.toBe(field("Name"));
  });

  it("leaves the focus alone after a save with the mouse that is the first thing done on the tab", async () => {
    // No keyboard action came before: the tab starts out as if the last one was from the keyboard, so
    // only Save itself can say that this click was a pointer's.
    await mount();
    await type(field("English"), "Welcome back");
    button("Save").focus();
    await pointer(button("Save"));
    await act(async () => {});
    expect(calls.filter((c) => c.command === "instance_brand.configure")).toHaveLength(1);
    expect(button("Save").disabled).toBe(true);
    expect(document.activeElement).not.toBe(field("Name"));
  });

  it("leaves the focus alone when the identity is removed with the mouse after a keyboard action", async () => {
    // Discard changes by the keyboard comes first and leaves the tab as "last used by the keyboard";
    // the click on Remove must say that the next change was started with a pointer.
    await mount();
    await type(field("English"), "Welcome back");
    button("Discard changes").focus();
    await click(button("Discard changes"));
    expect(document.activeElement).toBe(field("Name"));
    // A mouse puts the focus on the button it clicks; the button goes with the identity.
    button("Remove the identity").focus();
    await pointer(button("Remove the identity"));
    await act(async () => {});
    expect(calls.map((c) => c.command)).toContain("instance_brand.clear");
    expect(button("Remove the identity")).toBeUndefined();
    expect(document.activeElement).not.toBe(field("Name"));
  });

  it("leaves the focus where the person put it while the save was running", async () => {
    await mount();
    await type(field("Name"), "MOHV AB");
    let release!: () => void;
    hold = new Promise<void>((resolve) => (release = resolve));
    button("Save").focus();
    await click(button("Save"));
    field("English").focus();
    release();
    await act(async () => {});
    expect(calls.filter((c) => c.command === "instance_brand.configure")).toHaveLength(1);
    expect(document.activeElement).toBe(field("English"));
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

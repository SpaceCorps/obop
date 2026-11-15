// @vitest-environment jsdom
// The dialog that shows the one-time signing link, mounted for real (D92, docs/SIGN.md): a real
// DataProvider, the real SignLinkAction and Dialog, a fake `fetch` that answers like the server, and
// jsdom for the DOM. These tests drive it the way a person does (press the button, press Escape,
// click outside, copy) and read what is on the page and in the browser's state afterwards, so they
// fail when the logic is removed; the pure helpers are tested in signLinks.test.tsx.
import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { markActionOnly, type FieldMeta, type ModuleMeta, type Row } from "./api.ts";
import { DataProvider } from "./data.tsx";
import { heldLeaveQuestion } from "./leaveGuard.ts";
import { nav, useRoute, type Go } from "./route.ts";
import { SignLinkAction } from "./SignLinks.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TOKEN = "k".repeat(64);
const PATH = `/sign/req_1/${TOKEN}`;
const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const fields = [
  field("document_id", { kind: "ref", to: "documents_document" }),
  field("state", { kind: "select", options: ["draft", "sent", "signed", "declined", "expired"] }),
];
const request: Row = {
  id: "req-1",
  state: "sent",
  document_id: "doc-1",
  signer_name: "Ada Example",
};
const model = (name: string) =>
  ({
    name,
    description: "",
    can_create: true,
    can_bulk_create: true,
    can_update: true,
    can_delete: false,
    fields: [],
    actions: [],
  }) as ModuleMeta["models"][number];
const modules = (): ModuleMeta[] => {
  const m = [{ id: "sign", name: "Sign", models: [model("sign_request"), model("sign_link")] }];
  markActionOnly(m);
  return m;
};

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn>;
let fetched: { url: string; body: string }[];
/** Lets a held `sign_link.create` answer (see `fakeServer({ hold: true })`). */
let answerCreate: (() => void) | null;
let clipboard: { writeText: ReturnType<typeof vi.fn> };

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(status === 200 ? { value } : value), {
    status,
    headers: { "content-type": "application/json" },
  });

/** A fake server: `sign_link.create` answers with the link, every other call with an empty list. */
function fakeServer(opts: { hold?: boolean } = {}) {
  fetched = [];
  answerCreate = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      fetched.push({ url: String(url), body: String(init?.body ?? "") });
      if (String(url).includes("/api/sign_link/create")) {
        if (opts.hold) await new Promise<void>((done) => (answerCreate = done));
        return json({ id: "link-1", link: PATH, expires_at: "2030-01-01T00:00:00.000000Z" });
      }
      return json({ rows: [], total: 0 });
    }),
  );
}

async function mount(node: ReactNode = <Action />) {
  await act(async () => {
    root.render(node);
  });
}

function Action({ row = request }: { row?: Row }) {
  return (
    <DataProvider
      token="t"
      modules={modules()}
      apps={[]}
      notify={notify as never}
      confirm={async () => true}
      linkFor={() => null}
    >
      <SignLinkAction row={row} fields={fields} />
    </DataProvider>
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
const dialog = () => document.body.querySelector<HTMLElement>('[role="dialog"]');
const text = () => document.body.textContent ?? "";
const button = (name: string) =>
  [...document.body.querySelectorAll<HTMLElement>("button")].find(
    (b) => (b.textContent ?? "").trim() === name || b.getAttribute("aria-label") === name,
  );
const click = (el: Element | undefined) =>
  act(async () => {
    (el as HTMLElement).click();
  });
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
  });
const clickOutside = () =>
  act(async () => {
    document.body
      .querySelector(".overlay")!
      .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  });
const field_ = () => document.body.querySelector<HTMLInputElement>(".sign-link-field input")!;
const asked = () => text().includes("Close without copying?");

/** Press "Create signing link" and wait for the dialog. */
async function openDialog(node?: ReactNode) {
  await mount(node);
  await click(button("Create signing link"));
  for (let i = 0; i < 40 && !dialog(); i++) await settle();
  expect(dialog(), "the dialog did not open").not.toBeNull();
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  // Every test starts from the same blank browser state, so that a leak is seen as a change.
  localStorage.clear();
  sessionStorage.clear();
  window.name = "";
  document.title = "";
  history.replaceState(null, "", "/");
  notify = vi.fn();
  clipboard = { writeText: vi.fn(async () => {}) };
  Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
  nav.ask = null;
  nav.dirty = null;
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

describe("the dialog that shows the one-time link, mounted", () => {
  it("opens after the server's answer, shows the link in a read-only field and warns it is shown once", async () => {
    await openDialog();
    expect(field_().value).toBe(`${location.origin}${PATH}`);
    expect(field_().readOnly).toBe(true);
    expect(text()).toContain("Shown once");
    expect(text()).toContain("Anyone who has this link can sign for Ada Example.");
    const create = fetched.find((c) => c.url.includes("/api/sign_link/create"))!;
    expect(JSON.parse(create.body)).toEqual({ request_id: "req-1" });
  });

  it("starts with the focus on Copy, not on a Close button, and describes itself by the warning", async () => {
    await openDialog();
    expect(document.activeElement).toBe(button("Copy"));
    const note = document.getElementById(dialog()!.getAttribute("aria-describedby")!);
    expect(note?.textContent).toContain("Shown once");
  });

  it("asks before Escape lets the link go, and a second Escape goes back to the link", async () => {
    await openDialog();
    // The person has tabbed to Done when they press Escape.
    await act(async () => button("Done")!.focus());
    expect(document.activeElement).toBe(button("Done"));
    await press("Escape");
    expect(dialog(), "Escape closed an uncopied link").not.toBeNull();
    expect(asked()).toBe(true);
    // The question puts the focus on Copy, the safe button.
    expect(document.activeElement).toBe(button("Copy"));
    await press("Escape");
    expect(dialog()).not.toBeNull();
    expect(asked()).toBe(false);
    // Ten impatient presses never close it.
    for (let i = 0; i < 10; i++) await press("Escape");
    expect(dialog()).not.toBeNull();
  });

  it("asks the same way for a click outside, the X and Done", async () => {
    await openDialog();
    await clickOutside();
    expect(dialog()).not.toBeNull();
    expect(asked(), "a click outside did not ask").toBe(true);
    await press("Escape"); // back to the link
    expect(asked()).toBe(false);
    await click(button("Close"));
    expect(dialog(), "the X closed an uncopied link").not.toBeNull();
    expect(asked(), "the X did not ask").toBe(true);
    await press("Escape");
    await click(button("Done"));
    expect(dialog(), "Done closed an uncopied link").not.toBeNull();
    expect(asked(), "Done did not ask").toBe(true);
  });

  it("closes only through the button of the question, and the link is gone from the page", async () => {
    await openDialog();
    await press("Escape");
    await click(button("Close without copying"));
    expect(dialog()).toBeNull();
    expect(text()).not.toContain(TOKEN);
    expect(document.body.innerHTML).not.toContain(TOKEN);
  });

  it("copies the whole link with Copy, says Copied, and then closes without a question", async () => {
    await openDialog();
    await click(button("Copy"));
    await settle();
    expect(clipboard.writeText).toHaveBeenCalledTimes(1);
    expect(clipboard.writeText).toHaveBeenCalledWith(`${location.origin}${PATH}`);
    expect(document.body.querySelector('.sr-only[role="status"]')?.textContent).toBe("Copied");
    await press("Escape");
    expect(dialog(), "a copied link did not close on Escape").toBeNull();
  });

  it("takes the question away when the link is copied while it is shown, and then closes freely", async () => {
    await openDialog();
    await press("Escape");
    expect(asked()).toBe(true);
    expect(button("Done"), "Done is replaced by the question's button").toBeUndefined();
    await click(button("Copy"));
    await settle();
    expect(asked(), "the question stayed after the link was copied").toBe(false);
    expect(button("Done")).toBeDefined();
    await click(button("Done"));
    expect(dialog()).toBeNull();
  });

  it("closes through Done, the X and a click outside once the link was copied", async () => {
    for (const how of ["Done", "Close", "outside"]) {
      await openDialog();
      await click(button("Copy"));
      await settle();
      if (how === "outside") await clickOutside();
      else await click(button(how));
      expect(dialog(), `${how} did not close a copied link`).toBeNull();
    }
  });

  it("does not count a copy the browser refused: it says so and still asks before closing", async () => {
    clipboard.writeText.mockRejectedValue(new Error("denied"));
    await openDialog();
    await click(button("Copy"));
    await settle();
    expect(text()).toContain("Could not copy automatically");
    await press("Escape");
    expect(dialog()).not.toBeNull();
    expect(asked()).toBe(true);
  });

  it("counts a copy made by hand from the field, when the whole link was selected", async () => {
    await openDialog();
    // Focusing the field selects all of it, as a person clicking into it would.
    await act(async () => field_().focus());
    expect(field_().selectionStart).toBe(0);
    expect(field_().selectionEnd).toBe(field_().value.length);
    await act(async () => {
      field_().dispatchEvent(new Event("copy", { bubbles: true }));
    });
    await press("Escape");
    expect(dialog(), "a copy from the field did not release the dialog").toBeNull();
  });
});

describe("what happens to the link (state, not words)", () => {
  // The link is a secret that the server keeps only as a hash. Whatever the dialog does, the
  // browser must hold nothing of it afterwards: no storage, no cookie, no address, no title, no
  // window name, no notification, no console line, no request.
  async function cycle() {
    const before = {
      href: location.href,
      title: document.title,
      name: window.name,
      history: history.length,
    };
    const logged: unknown[][] = [];
    for (const level of ["log", "info", "warn", "error", "debug"] as const)
      vi.spyOn(console, level).mockImplementation((...args) => void logged.push(args));
    const beacon = vi.fn(() => true);
    Object.defineProperty(navigator, "sendBeacon", { value: beacon, configurable: true });
    await openDialog();
    await click(button("Copy"));
    await settle();
    await click(button("Done"));
    expect(dialog()).toBeNull();
    return { before, logged, beacon };
  }

  it("is nowhere in the browser's state after the dialog was used and closed", async () => {
    const { before, logged, beacon } = await cycle();
    const link = `${location.origin}${PATH}`;
    const hold = (v: unknown) => JSON.stringify(v) ?? "";
    expect(localStorage.length, "localStorage").toBe(0);
    expect(sessionStorage.length, "sessionStorage").toBe(0);
    expect(document.cookie, "cookie").toBe("");
    expect(location.href, "the address").toBe(before.href);
    expect(history.length, "history").toBe(before.history);
    expect(document.title, "the title").toBe(before.title);
    expect(window.name, "window.name").toBe(before.name);
    expect(hold(history.state), "history.state").not.toContain(TOKEN);
    expect(hold(notify.mock.calls), "a notification").not.toContain(TOKEN);
    expect(hold(logged), "the console").not.toContain(TOKEN);
    expect(beacon, "sendBeacon").not.toHaveBeenCalled();
    expect(document.body.innerHTML, "the page").not.toContain(TOKEN);
    // The only request is the one that makes the link, and it does not carry the link.
    expect(fetched.map((c) => c.url.replace(/\?.*$/, ""))).toEqual(["/api/sign_link/create"]);
    for (const c of fetched) expect(`${c.url} ${c.body}`).not.toContain(link.slice(-20));
  });

  it("is not in a notification, the console or storage while it is shown either", async () => {
    await openDialog();
    expect(field_().value).toContain(TOKEN); // it is on the page, in the dialog, and only there
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
    expect(notify).not.toHaveBeenCalled();
    expect(document.title).not.toContain(TOKEN);
    expect(location.href).not.toContain(TOKEN);
  });
});

describe("leaving the page while the link is shown (Back, a reload, the palette, the panel)", () => {
  // The dialog lives inside the record panel, which lives under the route: anything that changes
  // the route takes the dialog, and the link, away with it.
  let go: Go;
  function Page() {
    const [route, g] = useRoute();
    go = g;
    return route.model === "" || route.model === "sign_request" ? <Action /> : <p>elsewhere</p>;
  }
  const leavePrompt = () => {
    const e = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(e);
    return e.defaultPrevented;
  };
  const answerWith = (leave: boolean) => {
    const ask = vi.fn(async (_message: string, _leave: string, _stay: string) => leave);
    nav.ask = ask;
    return ask;
  };
  const untilAsked = async (ask: ReturnType<typeof vi.fn>) => {
    for (let i = 0; i < 30 && !ask.mock.calls.length; i++) await settle();
  };

  it("asks the browser before a reload or a closed tab until the link was copied", async () => {
    await openDialog();
    expect(leavePrompt(), "no prompt while the link is shown and not copied").toBe(true);
    await click(button("Copy"));
    await settle();
    expect(leavePrompt(), "a prompt although the link was copied").toBe(false);
  });

  it("gives the prompt up when the dialog is closed without copying", async () => {
    await openDialog();
    await press("Escape");
    await click(button("Close without copying"));
    expect(dialog()).toBeNull();
    expect(leavePrompt()).toBe(false);
    expect(heldLeaveQuestion()).toBeNull();
  });

  it("holds a question for the route while the link is shown and not copied, and drops it after the copy", async () => {
    await openDialog();
    const q = heldLeaveQuestion();
    expect(q?.message).toContain("has not been copied");
    expect(q?.leave).toBe("Leave without the link");
    expect(q?.stay).toBe("Stay and copy it");
    await click(button("Copy"));
    await settle();
    expect(heldLeaveQuestion()).toBeNull();
  });

  it("asks before the browser's Back or a swipe changes the address, and puts the address back", async () => {
    const ask = answerWith(false);
    await openDialog(<Page />);
    const here = location.hash;
    await act(async () => {
      location.hash = "#/crm/lead";
    });
    await untilAsked(ask);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask.mock.calls[0]![1]).toBe("Leave without the link");
    await settle();
    expect(location.hash, "the address was not put back").toBe(here || "#/");
    expect(dialog(), "the dialog went away although they stayed").not.toBeNull();
    expect(text()).not.toContain("elsewhere");
  });

  it("lets go of the dialog, and only then, when the person says leave", async () => {
    const ask = answerWith(true);
    await openDialog(<Page />);
    await act(async () => {
      location.hash = "#/crm/lead";
    });
    await untilAsked(ask);
    for (let i = 0; i < 30 && dialog(); i++) await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(dialog()).toBeNull();
    expect(text()).toContain("elsewhere");
  });

  it("asks before the palette or the panel's close button navigates in code", async () => {
    const ask = answerWith(false);
    await openDialog(<Page />);
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(dialog()).not.toBeNull();
    expect(text()).not.toContain("elsewhere");
  });

  it("does not ask when the address does not change (Save, Ctrl+S while the link is shown)", async () => {
    const ask = answerWith(false);
    await openDialog(<Page />);
    // Saving the form goes to the record's own address, which is the one on screen.
    await act(async () => go({}));
    await settle();
    expect(ask).not.toHaveBeenCalled();
    expect(dialog(), "the dialog went away").not.toBeNull();
    expect(asked()).toBe(false);
  });

  it("does not ask once the link was copied", async () => {
    const ask = answerWith(false);
    await openDialog(<Page />);
    await click(button("Copy"));
    await settle();
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).not.toHaveBeenCalled();
    expect(text()).toContain("elsewhere");
  });

  // The form under the dialog can hold unsaved edits as well (a sent request is editable, and the
  // button stays enabled while the form is dirty). One question has to name both things.
  it("names the unsaved changes and the uncopied link in one question", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await openDialog(<Page />);
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    const [message, leave, stay] = ask.mock.calls[0]!;
    expect(message).toContain("unsaved changes");
    expect(message).toContain("signing link has not been copied");
    expect(leave).toBe("Leave anyway");
    expect(stay).toBe("Stay");
    expect(dialog()).not.toBeNull();
    expect(text()).not.toContain("elsewhere");
  });

  it("asks only about the changes once the link was copied", async () => {
    const ask = answerWith(false);
    nav.dirty = () => true;
    await openDialog(<Page />);
    await click(button("Copy"));
    await settle();
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask.mock.calls[0]![0]).toBe("You have unsaved changes. Leave and discard them?");
    expect(ask.mock.calls[0]![1]).toBe("Leave and discard");
  });

  it("names the unsaved changes and the link that is still being made", async () => {
    fakeServer({ hold: true });
    const ask = answerWith(false);
    nav.dirty = () => true;
    await mount(<Page />);
    await click(button("Create signing link"));
    await settle();
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    const [message, leave, stay] = ask.mock.calls[0]!;
    expect(message).toContain("unsaved changes");
    expect(message).toContain("still being made");
    expect([leave, stay]).toEqual(["Leave anyway", "Stay"]);
    await act(async () => answerCreate!());
    for (let i = 0; i < 40 && !dialog(); i++) await settle();
  });

  it("asks before leaving while the server is still making the link, then holds the link's own question", async () => {
    fakeServer({ hold: true });
    const ask = answerWith(false);
    await mount(<Page />);
    await click(button("Create signing link"));
    await settle();
    expect(answerCreate, "the request was not sent").not.toBeNull();
    expect(heldLeaveQuestion()?.message).toContain("still being made");
    expect(leavePrompt()).toBe(true);
    await act(async () => go({ app: "crm", model: "lead" }));
    await settle();
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask.mock.calls[0]![0]).toContain("still being made");
    // The answer arrives: the dialog opens and the question is now the link's.
    await act(async () => answerCreate!());
    for (let i = 0; i < 40 && !dialog(); i++) await settle();
    expect(dialog()).not.toBeNull();
    expect(heldLeaveQuestion()?.message).toContain("has not been copied");
  });

  it("tells the person, when they left anyway, that the link was made and nobody has it", async () => {
    fakeServer({ hold: true });
    await mount();
    await click(button("Create signing link"));
    await settle();
    await act(async () => root.unmount());
    root = createRoot(container);
    await act(async () => answerCreate!());
    await settle();
    expect(dialog(), "a dialog opened in a panel that is gone").toBeNull();
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0]![0]).toContain("you left before it was shown");
    expect(notify.mock.calls[0]![0]).not.toContain(TOKEN);
    expect(heldLeaveQuestion()).toBeNull();
  });

  it("counts a copy by hand only when the whole link was selected", async () => {
    const copyByHand = async (select: (input: HTMLInputElement) => void) => {
      await act(async () => {
        select(field_());
        field_().dispatchEvent(new Event("copy", { bubbles: true }));
      });
    };
    await openDialog();
    const parts: [string, (i: HTMLInputElement) => void][] = [
      ["a fragment in the middle", (i) => i.setSelectionRange(3, 12)],
      ["the start of it", (i) => i.setSelectionRange(0, 12)],
      ["the end of it", (i) => i.setSelectionRange(3, i.value.length)],
      ["only a caret at the start", (i) => i.setSelectionRange(0, 0)],
      ["only a caret at the end", (i) => i.setSelectionRange(i.value.length, i.value.length)],
    ];
    for (const [what, select] of parts) {
      await copyByHand(select);
      expect(heldLeaveQuestion(), `${what} released the leave question`).not.toBeNull();
      await press("Escape");
      expect(dialog(), `${what} released the dialog`).not.toBeNull();
      expect(asked(), `${what}: no question`).toBe(true);
      await press("Escape"); // back to the link
    }
    // The whole link.
    await copyByHand((i) => i.select());
    expect(heldLeaveQuestion()).toBeNull();
    await press("Escape");
    expect(dialog()).toBeNull();
  });
});

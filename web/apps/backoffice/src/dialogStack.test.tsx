// @vitest-environment jsdom
// Dialogs that stack (D92 re-check W2). The app has three slots that open above whatever dialog is
// already open: the question the shell asks (`confirm`, `nav.ask`: Back with an unsaved form, leaving
// while a signing link is unkept), the command palette (Ctrl+K) and the app launcher. Escape belongs
// to the topmost dialog only: it must not also reach the one underneath. The signing-link dialog is
// the case that hurt: Escape on the leave question also made it show "Close without copying?", and
// Escape on the palette over a copied link closed the link dialog as well.
//
// These tests mount the real Dialog, the real ConfirmDialog and Palette and the real link dialog with
// the wiring of App.tsx (one confirm slot, `nav.ask = confirm`, `useRoute`).
import { act, memo, useCallback, useEffect, useState } from "react";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { Dialog, provideCatalogue, setLanguage } from "@obop/ui";
import { markActionOnly, type FieldMeta, type ModuleMeta, type Row } from "./api.ts";
import { Palette } from "./Chrome.tsx";
import { DataProvider } from "./data.tsx";
import { nav, useRoute } from "./route.ts";
import { SignLinkAction } from "./SignLinks.tsx";
import { ConfirmDialog, type ConfirmRequest } from "./ui.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TOKEN = "k".repeat(64);
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
const request: Row = { id: "req-1", state: "sent", document_id: "doc-1", signer_name: "Ada" };
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
let openPalette: () => void;

/** The slots of App.tsx: a page, the one confirm slot wired to `nav.ask`, and the palette. */
function Shell() {
  const [route] = useRoute();
  const [confirmReq, setConfirmReq] = useState<ConfirmRequest | null>(null);
  const [palette, setPalette] = useState(false);
  openPalette = () => setPalette(true);
  const confirm = useCallback(
    (message: string, confirmLabel = "Discard", cancelLabel = "Keep editing") =>
      new Promise<boolean>((resolve) =>
        setConfirmReq({
          message,
          confirmLabel,
          cancelLabel,
          resolve: (ok) => {
            setConfirmReq(null);
            resolve(ok);
          },
        }),
      ),
    [],
  );
  useEffect(() => {
    nav.ask = (message, leave, stay) => confirm(message, leave, stay);
    return () => {
      nav.ask = null;
    };
  }, [confirm]);
  return (
    <DataProvider
      token="t"
      modules={modules()}
      apps={[]}
      notify={() => {}}
      confirm={confirm}
      linkFor={() => null}
    >
      {route.model === "" || route.model === "sign_request" ? (
        <SignLinkAction row={request} fields={fields} />
      ) : (
        <p>elsewhere</p>
      )}
      {palette && <Palette apps={[]} onClose={() => setPalette(false)} />}
      {confirmReq && <ConfirmDialog req={confirmReq} />}
    </DataProvider>
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
const text = () => document.body.textContent ?? "";
const button = (name: string) =>
  [...document.body.querySelectorAll<HTMLElement>("button")].find(
    (b) => (b.textContent ?? "").trim() === name || b.getAttribute("aria-label") === name,
  );
const click = (el: Element | undefined) =>
  act(async () => {
    (el as HTMLElement).click();
  });
const escape = () =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
  });
const linkDialog = () => document.body.querySelector('[role="dialog"]:not(.palette)');
const linkField = () =>
  document.body.querySelector<HTMLInputElement>(".sign-link-field input")?.value;
const question = () => document.body.querySelector('[role="alertdialog"]');
const palette = () => document.body.querySelector(".palette");
const asked = () => text().includes("Close without copying?");

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  history.replaceState(null, "", "/");
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: vi.fn(async () => {}) },
    configurable: true,
  });
  nav.ask = null;
  nav.dirty = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const body = String(url).includes("/api/sign_link/create")
        ? { id: "link-1", link: `/sign/req_1/${TOKEN}`, expires_at: "2030-01-01T00:00:00.000000Z" }
        : { rows: [], total: 0 };
      return new Response(JSON.stringify({ value: body }), {
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
  vi.restoreAllMocks();
});

/** Show the link (not copied yet). */
async function showLink() {
  await act(async () => {
    root.render(<Shell />);
  });
  await click(button("Create signing link"));
  for (let i = 0; i < 40 && !linkDialog(); i++) await settle();
  expect(linkDialog(), "the link dialog did not open").not.toBeNull();
}

describe("the question the shell asks above the signing-link dialog (Back with an uncopied link)", () => {
  async function back() {
    await act(async () => {
      location.hash = "#/crm/lead";
    });
    for (let i = 0; i < 40 && !question(); i++) await settle();
    expect(question(), "the leave question did not open").not.toBeNull();
  }

  it("Escape answers the question alone and does not make the link dialog ask as well", async () => {
    await showLink();
    await back();
    expect(text()).toContain("has not been copied");
    await escape();
    await settle();
    expect(question(), "the leave question is still open").toBeNull();
    expect(linkDialog(), "the link dialog went away").not.toBeNull();
    expect(asked(), "the link dialog asked a question nobody triggered").toBe(false);
    expect(text(), "the person stayed").not.toContain("elsewhere");
  });

  it("the next Escape then belongs to the link dialog, which asks", async () => {
    await showLink();
    await back();
    await escape();
    await settle();
    await escape();
    expect(asked()).toBe(true);
    expect(linkDialog()).not.toBeNull();
  });
});

describe("the command palette opened above the signing-link dialog", () => {
  it("Escape closes the palette alone while the link is not copied", async () => {
    await showLink();
    await act(async () => openPalette());
    expect(palette()).not.toBeNull();
    await escape();
    expect(palette(), "the palette is still open").toBeNull();
    expect(linkDialog()).not.toBeNull();
    expect(asked(), "the link dialog asked a question nobody triggered").toBe(false);
  });

  it("Escape closes the palette alone after the copy, the link dialog stays", async () => {
    await showLink();
    await click(button("Copy"));
    await settle();
    await act(async () => openPalette());
    expect(palette()).not.toBeNull();
    await escape();
    expect(palette()).toBeNull();
    expect(linkDialog(), "Escape on the palette closed the link dialog too").not.toBeNull();
    expect(linkField()).toContain(TOKEN);
    // And the one after that is the link dialog's own.
    await escape();
    expect(linkDialog()).toBeNull();
  });
});

describe("the Dialog primitive, stacked", () => {
  let closed: number[];
  beforeEach(() => {
    closed = [];
  });
  const render = (node: React.ReactNode) =>
    act(async () => {
      root.render(node);
    });

  /** Three dialogs, each logging its own closing; `open` says which are open. */
  function Stack({ open, tick = 0 }: { open: boolean[]; tick?: number }) {
    return (
      <>
        {open.map((o, i) => (
          // A fresh onClose on every render, as inline arrows are: the listener is re-registered.
          <Dialog key={i} open={o} title={`Dialog ${i}`} onClose={() => closed.push(i)}>
            <button>
              inside {i} {tick}
            </button>
          </Dialog>
        ))}
      </>
    );
  }

  it("Escape reaches the topmost dialog only", async () => {
    await render(<Stack open={[true, true, true]} />);
    await escape();
    expect(closed).toEqual([2]);
  });

  it("the next dialog down gets the next Escape once the top one is gone", async () => {
    await render(<Stack open={[true, true]} />);
    await escape();
    await render(<Stack open={[true, false]} />);
    await escape();
    expect(closed).toEqual([1, 0]);
  });

  it("a dialog that is not open does not take Escape from the one below", async () => {
    await render(<Stack open={[true, false]} />);
    await escape();
    expect(closed).toEqual([0]);
  });

  // The dialog underneath renders again after the one above opened (a state change of its own), so
  // its onClose is a new function and its listener is now the later one on the window: it must still
  // be the one underneath, and the one above must still answer first.
  let rerenderBelow: () => void;
  let openAbove: (open: boolean) => void;
  const Below = memo(function Below() {
    const [n, setN] = useState(0);
    rerenderBelow = () => setN((v) => v + 1);
    return (
      <Dialog title="below" onClose={() => closed.push(0)}>
        <button>below {n}</button>
      </Dialog>
    );
  });
  function Pair({ sync }: { sync: boolean }) {
    const [above, setAbove] = useState(false);
    openAbove = setAbove;
    return (
      <>
        <Below />
        {above && (
          <Dialog
            title="above"
            onClose={() => {
              closed.push(1);
              // Closing at once, inside the key event, as a dialog that answers a promise can.
              if (sync) flushSync(() => setAbove(false));
              else setAbove(false);
            }}
          />
        )}
      </>
    );
  }

  it("a dialog underneath that rendered again after the one above opened is still underneath", async () => {
    await render(<Pair sync={false} />);
    await act(async () => openAbove(true));
    await act(async () => rerenderBelow());
    await escape();
    expect(closed).toEqual([1]);
  });

  it("a dialog above that closes at once, inside the key event, does not hand the same Escape down", async () => {
    await render(<Pair sync />);
    await act(async () => openAbove(true));
    await act(async () => rerenderBelow());
    await escape();
    expect(closed).toEqual([1]);
    // The one underneath answers the next Escape.
    await escape();
    expect(closed).toEqual([1, 0]);
  });
});

describe("a dialog that takes Escape stops it there (WEB3-1)", () => {
  // The record panel closes on an Escape that reaches the window while no overlay is on the page
  // (RecordPanel.tsx). A question above the panel is an overlay: Escape answers it, the overlay goes,
  // and only the dialog's stopPropagation keeps that same press from closing the panel as well.
  const render = (node: React.ReactNode) =>
    act(async () => {
      root.render(node);
    });
  const key = () =>
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );

  it("reaches no listener further along: not the target, the document, nor the window's own", async () => {
    const heard: string[] = [];
    const target = () => heard.push("target");
    const doc = () => heard.push("document");
    const win = () => heard.push("window");
    const closed: string[] = [];
    await render(
      <Dialog title="one" onClose={() => closed.push("one")}>
        <button>inside</button>
      </Dialog>,
    );
    const inside = document.body.querySelector("button")!;
    inside.addEventListener("keydown", target);
    document.addEventListener("keydown", doc);
    window.addEventListener("keydown", win); // a bubble listener on the window, like RecordPanel's
    try {
      await act(async () => void key());
    } finally {
      inside.removeEventListener("keydown", target);
      document.removeEventListener("keydown", doc);
      window.removeEventListener("keydown", win);
    }
    expect(closed).toEqual(["one"]);
    expect(heard, "the Escape went on after the dialog took it").toEqual([]);
  });

  it("does not stop a key that is not Escape", async () => {
    const heard: string[] = [];
    const win = () => heard.push("window");
    await render(<Dialog title="one" onClose={() => {}} />);
    window.addEventListener("keydown", win);
    try {
      await act(async () =>
        document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true })),
      );
    } finally {
      window.removeEventListener("keydown", win);
    }
    expect(heard).toEqual(["window"]);
  });

  it("is what keeps the record panel open when the question above it is answered with Escape", async () => {
    // A browser runs the update between two listeners of one event; jsdom does not. flushSync puts
    // the question away inside the first listener, as that would, so the panel's listener finds no
    // overlay left on the page unless the event was stopped.
    const panelClosed = vi.fn();
    function Panel() {
      const [open, setOpen] = useState(true);
      useEffect(() => {
        const on = (e: KeyboardEvent) => {
          if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".overlay"))
            panelClosed();
        };
        window.addEventListener("keydown", on);
        return () => window.removeEventListener("keydown", on);
      }, []);
      return (
        <>
          <p>the panel</p>
          {open && (
            <Dialog
              title="Confirm"
              role="alertdialog"
              onClose={() => flushSync(() => setOpen(false))}
            >
              <button>Keep it</button>
            </Dialog>
          )}
        </>
      );
    }
    await render(<Panel />);
    expect(document.body.querySelector(".overlay")).not.toBeNull();
    const act_ = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
    act_.IS_REACT_ACT_ENVIRONMENT = false; // the update is made outside act on purpose
    try {
      key();
    } finally {
      act_.IS_REACT_ACT_ENVIRONMENT = true;
    }
    expect(document.body.querySelector(".overlay"), "the question is still open").toBeNull();
    expect(panelClosed, "the same Escape closed the panel too").not.toHaveBeenCalled();
    // The next Escape, with no question above it, is the panel's.
    await act(async () => void key());
    expect(panelClosed).toHaveBeenCalledTimes(1);
  });
});

describe("the stack follows the order the dialogs are drawn in (WEB3-2)", () => {
  // Several dialogs can be committed together. A dialog that is written inside another dialog's
  // children is committed before its parent, in the page as in the stack, so the one drawn last (on
  // top) is the one that answers Escape in every arrangement.
  const render = (node: React.ReactNode) =>
    act(async () => {
      root.render(node);
    });
  const order = () =>
    [...document.body.querySelectorAll<HTMLElement>(".overlay h2")].map((h) => h.textContent);
  const closed: string[] = [];
  beforeEach(() => {
    closed.length = 0;
  });
  const D = ({ name, children }: { name: string; children?: React.ReactNode }) => (
    <Dialog bare title={name} onClose={() => closed.push(name)}>
      {children}
    </Dialog>
  );

  it("a dialog inside another one's children, both open at once: the one drawn last answers", async () => {
    await render(
      <D name="outer">
        <D name="inner" />
      </D>,
    );
    // React commits the inner one first, in the page and in the stack: the outer one is on top.
    expect(order()).toEqual(["inner", "outer"]);
    await act(async () => void escapeKey());
    expect(closed).toEqual(["outer"]);
  });

  it("siblings opened together: the one drawn last answers", async () => {
    await render(
      <>
        <D name="first" />
        <D name="second" />
      </>,
    );
    const drawn = order();
    await act(async () => void escapeKey());
    expect(closed).toEqual([drawn[1]]);
  });

  it("a dialog inside another one's children that opens later: it is drawn last and answers", async () => {
    let show: (v: boolean) => void;
    function Outer() {
      const [on, setOn] = useState(false);
      show = setOn;
      return <D name="outer">{on && <D name="inner" />}</D>;
    }
    await render(<Outer />);
    await act(async () => show(true));
    expect(order()).toEqual(["outer", "inner"]);
    await act(async () => void escapeKey());
    expect(closed).toEqual(["inner"]);
  });
});

const escapeKey = () =>
  (document.activeElement ?? document.body).dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
  );

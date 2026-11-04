// @vitest-environment jsdom
// The board and the list of sign requests, mounted (D92): a request about a Document is not moved to Signed by
// hand, because the signer signs it through the link and the server refuses the manual step. The
// card's "Move to" menu and the state cell of the list must grey that step out, and a move that is
// tried anyway must be refused with the reason and send nothing; a request from a template keeps it.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, ModuleMeta, Row } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { emptyState } from "../query.ts";
import { KanbanView } from "./KanbanView.tsx";
import { ListView } from "./ListView.tsx";
import type { ViewProps } from "./types.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const state = field("state", {
  kind: "select",
  required: true,
  options: ["draft", "sent", "signed", "declined", "expired"],
});
const action = (name: string, from: string, to: string): Command => ({
  id: `sign_request.${name}`,
  description: name,
  needs_approval: false,
  field: "state",
  from,
  to,
});
const model: ModelMeta = {
  name: "sign_request",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    field("title", { required: true }),
    state,
    field("document_id", { kind: "ref", to: "documents_document" }),
    field("template_id", { kind: "ref", to: "sign_template" }),
    field("document_hash"),
  ],
  actions: [
    action("send", "draft", "sent"),
    action("sign", "sent", "signed"),
    action("decline", "sent", "declined"),
    action("send_document", "draft", "sent"),
  ],
};
const modules: ModuleMeta[] = [{ id: "sign", name: "Sign", models: [model] }];

const rows: Row[] = [
  { id: "doc", title: "Document request", state: "sent", document_id: "d1" },
  {
    id: "cleared",
    title: "Cleared request",
    state: "sent",
    document_id: null,
    document_hash: "ab12",
  },
  { id: "tpl", title: "Template request", state: "sent", template_id: "t1" },
  {
    id: "legacy",
    title: "Typed hash request",
    state: "sent",
    template_id: "t1",
    document_hash: "ab12",
  },
];

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn>;
let fetch_: ReturnType<typeof vi.fn>;

async function mount(View: (p: ViewProps) => React.ReactNode) {
  const props: ViewProps = {
    model,
    title: "Request",
    fields: model.fields,
    rows,
    state: { ...emptyState(), group: ["state"] },
    setState: () => {},
    selectedId: "",
    onOpen: () => {},
    canCreate: true,
    canUpdate: true,
    update: async () => null,
    create: async () => null,
    moveState: async () => null,
    total: rows.length,
    fetchAll: async () => ({ rows: [], total: 0, complete: true }) as never,
    canDelete: true,
    setDialog: () => {},
    newWith: () => {},
    drill: () => {},
  };
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={modules}
        apps={[]}
        notify={notify as never}
        confirm={async () => true}
        linkFor={() => null}
      >
        <View {...props} />
      </DataProvider>,
    );
  });
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  notify = vi.fn();
  fetch_ = vi.fn(async () => new Response(JSON.stringify({ value: { rows: [], total: 0 } })));
  vi.stubGlobal("fetch", fetch_);
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

const menu = (id: string) =>
  container.querySelector<HTMLSelectElement>(`[data-card="${id}"] select.moveto`)!;
const option = (id: string, label: string) =>
  [...menu(id).options].find((o) => o.textContent?.trim().startsWith(label))!;
const REASON =
  "A request about a document is signed by the signer through the signing link. It cannot be marked signed by hand.";

describe("the 'Move to' menu of a card on the sign board", () => {
  beforeEach(() => mount(KanbanView));

  it("greys out Signed, with the reason, for a request about a document", () => {
    expect(option("doc", "Signed").disabled).toBe(true);
    expect(option("doc", "Signed").title).toBe(REASON);
  });

  it("greys it out for a request that was left holding only a document's hash", () => {
    expect(option("cleared", "Signed").disabled).toBe(true);
    expect(option("cleared", "Signed").title).toBe(REASON);
  });

  it("keeps Signed for a request from a template, also one with a hash typed by hand", () => {
    expect(option("tpl", "Signed").disabled).toBe(false);
    expect(option("legacy", "Signed").disabled).toBe(false);
  });

  it("does not touch the other steps of a request about a document", () => {
    expect(option("doc", "Declined").disabled).toBe(false);
  });
});

describe("a card dragged over the columns", () => {
  beforeEach(() => mount(KanbanView));

  const transfer = () => ({
    dropEffect: "move",
    effectAllowed: "all",
    setData: () => {},
    getData: () => "",
  });
  const fire = (el: Element, type: string, dt = transfer()) => {
    const e = new Event(type, { bubbles: true, cancelable: true });
    Object.assign(e, { dataTransfer: dt });
    el.dispatchEvent(e);
    return { e, dt };
  };
  const column = (name: string) =>
    [...container.querySelectorAll<HTMLElement>("section.kcol")].find((c) =>
      (c.getAttribute("aria-label") ?? "").startsWith(`${name},`),
    )!;
  const pickUp = (id: string) =>
    act(async () => void fire(container.querySelector(`[data-card="${id}"]`)!, "dragstart"));
  const dragOver = async (name: string) => {
    let out!: ReturnType<typeof fire>;
    await act(async () => void (out = fire(column(name), "dragover")));
    return out;
  };

  it("is not accepted over Signed for a request about a document, and says why", async () => {
    await pickUp("doc");
    const { e, dt } = await dragOver("Signed");
    expect(e.defaultPrevented, "the column took the card").toBe(false);
    expect(dt.dropEffect).toBe("none");
    expect(column("Signed").className).toContain("blocked");
    expect(column("Signed").title).toBe(REASON);
  });

  it("is not accepted for a request that was left holding only a document's hash", async () => {
    await pickUp("cleared");
    expect((await dragOver("Signed")).e.defaultPrevented).toBe(false);
    expect(column("Signed").className).toContain("blocked");
  });

  it("is accepted over Declined, which the request may still be moved to", async () => {
    await pickUp("doc");
    const { e } = await dragOver("Declined");
    expect(e.defaultPrevented).toBe(true);
    expect(column("Declined").className).not.toContain("blocked");
  });

  it("is accepted over Signed for a request from a template", async () => {
    await pickUp("tpl");
    expect((await dragOver("Signed")).e.defaultPrevented).toBe(true);
    expect(column("Signed").className).not.toContain("blocked");
  });
});

describe("a move that is tried anyway", () => {
  beforeEach(() => mount(KanbanView));

  const tryMove = async (id: string) => {
    await act(async () => {
      menu(id).value = "signed";
      menu(id).dispatchEvent(new Event("change", { bubbles: true }));
    });
    for (let i = 0; i < 5; i++) await act(async () => void (await Promise.resolve()));
  };

  it("is refused with the reason, and nothing is sent, for a request about a document", async () => {
    await tryMove("doc");
    expect(notify).toHaveBeenCalledWith(REASON, "error");
    expect(
      fetch_.mock.calls.filter(([url]) => String(url).includes("/api/sign_request/sign")),
    ).toEqual([]);
  });

  it("is sent as the sign action for a request from a template", async () => {
    await tryMove("tpl");
    const calls = fetch_.mock.calls.filter(([url]) =>
      String(url).includes("/api/sign_request/sign"),
    );
    expect(calls).toHaveLength(1);
    expect(notify).not.toHaveBeenCalledWith(REASON, "error");
  });
});

describe("the state cell of a row in the list of sign requests", () => {
  beforeEach(() => mount(ListView));

  /** Click the State cell of the row, which edits it in place, and read the options of its menu. */
  const states = async (title: string) => {
    const row = [...container.querySelectorAll("tr")].find((r) => r.textContent?.includes(title))!;
    const cell = [...row.querySelectorAll("td")].find((c) =>
      /Sent/.test(c.textContent ?? ""),
    ) as HTMLElement;
    await act(async () => cell.click());
    const select = row.querySelector<HTMLSelectElement>(".inline-edit select")!;
    return [...select.options];
  };
  const signed = (options: HTMLOptionElement[]) =>
    options.find((o) => o.textContent?.trim().startsWith("Signed"))!;

  it("greys out Signed, with the reason, for a request about a document", async () => {
    const option = signed(await states("Document request"));
    expect(option.disabled).toBe(true);
    expect(option.title).toBe(REASON);
  });

  it("greys it out for a request that was left holding only a document's hash", async () => {
    expect(signed(await states("Cleared request")).disabled).toBe(true);
  });

  it("keeps Signed for a request from a template, also one with a hash typed by hand", async () => {
    expect(signed(await states("Template request")).disabled).toBe(false);
    expect(signed(await states("Typed hash request")).disabled).toBe(false);
  });
});

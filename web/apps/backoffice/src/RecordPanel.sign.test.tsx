// @vitest-environment jsdom
// The record panel of a sign request, mounted (D92 re-check round 3, W1, W4, W6). A request about a
// Document is signed by the signer through the signing link, and the server refuses the manual
// "sign": so the panel has no Sign button for it, its Signed step is greyed out with the reason, and
// a note says how it is signed. A request from a template keeps all three. These are the wiring in
// RecordPanel.tsx (the action row, the status bar, the note) that no other test reaches; the pure
// rules are in workflow.test.ts and the board and list in views/signBoard.test.tsx.
import { act, useCallback, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import {
  markActionOnly,
  type Command,
  type FieldMeta,
  type ModelMeta,
  type ModuleMeta,
  type Row,
} from "./api.ts";
import { DataProvider } from "./data.tsx";
import { nav } from "./route.ts";
import { RecordPanel } from "./RecordPanel.tsx";
import { ConfirmDialog, type ConfirmRequest } from "./ui.tsx";

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
const action = (name: string, from: string, to: string): Command => ({
  id: `sign_request.${name}`,
  description: name,
  needs_approval: false,
  field: "state",
  from,
  to,
});
const request: ModelMeta = {
  name: "sign_request",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    field("title", { required: true }),
    field("state", {
      kind: "select",
      required: true,
      options: ["draft", "sent", "signed", "declined", "expired"],
    }),
    field("signer_name"),
    field("document_id", { kind: "ref", to: "documents_document" }),
    field("template_id", { kind: "ref", to: "sign_template" }),
    field("document_hash"),
  ],
  actions: [
    action("send", "draft", "sent"),
    action("send_document", "draft", "sent"),
    action("sign", "sent", "signed"),
    action("decline", "sent", "declined"),
  ],
};
const links = (can: { make: boolean; revoke: boolean }): ModelMeta => ({
  name: "sign_link",
  description: "",
  can_create: can.make,
  can_update: can.revoke,
  can_delete: false,
  fields: [
    field("request_id", { kind: "ref", to: "sign_request" }),
    field("expires_at", { kind: "datetime" }),
    field("used_at", { kind: "datetime" }),
    field("revoked_at", { kind: "datetime" }),
  ],
  actions: [],
});
const plain = (name: string): ModelMeta => ({
  name,
  description: "",
  can_create: false,
  can_update: false,
  can_delete: false,
  fields: [field("title")],
  actions: [],
});
const modulesFor = (can: { make: boolean; revoke: boolean }): ModuleMeta[] => {
  const m: ModuleMeta[] = [
    { id: "sign", name: "Sign", models: [request, links(can), plain("sign_template")] },
    { id: "documents", name: "Documents", models: [plain("documents_document")] },
  ];
  markActionOnly(m);
  return m;
};

const REASON =
  "A request about a document is signed by the signer through the signing link. It cannot be marked signed by hand.";
const base = { id: "r1", title: "NDA with Ada Example", signer_name: "Ada Example" };
const rows = {
  document: { ...base, state: "sent", document_id: "d1", document_hash: "ab12" },
  documentDraft: { ...base, state: "draft", document_id: "d1" },
  documentSigned: { ...base, state: "signed", document_id: "d1", document_hash: "ab12" },
  cleared: { ...base, state: "sent", document_id: null, document_hash: "ab12" },
  template: { ...base, state: "sent", template_id: "t1" },
  templateDraft: { ...base, state: "draft", template_id: "t1" },
  typedHash: { ...base, state: "sent", template_id: "t1", document_hash: "ab12" },
} satisfies Record<string, Row>;

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn>;
let calls: { url: string; body: unknown }[];
let linkRows: Row[];
/** What `sign_link.create` answers. */
let createAnswer: unknown;

const json = (value: unknown) =>
  new Response(JSON.stringify({ value }), { headers: { "content-type": "application/json" } });

/** A fake server: the links of the request are `linkRows`, a command answers with its record. */
function fakeServer() {
  calls = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const u = String(url).replace(/\?.*$/, "");
      calls.push({ url: u, body: init?.body ? JSON.parse(String(init.body)) : null });
      if (u === "/api/sign_link/search") return json({ rows: linkRows, total: linkRows.length });
      if (u === "/api/sign_link/create") return json(createAnswer);
      if (/\/api\/sign_(request|link)\/(sign|decline|send|send_document|update)$/.test(u))
        return json({ id: "r1" });
      return json({ rows: [], total: 0 });
    }),
  );
}
const sent = (command: string) => calls.filter((c) => c.url === `/api/${command}`);

/** What App.tsx has around the panel: the one confirm slot, which the Revoke question uses. */
function Shell({ row, can }: { row: Row; can: { make: boolean; revoke: boolean } }) {
  const [req, setReq] = useState<ConfirmRequest | null>(null);
  const confirm = useCallback(
    (message: string, confirmLabel = "Discard", cancelLabel = "Keep editing") =>
      new Promise<boolean>((resolve) =>
        setReq({
          message,
          confirmLabel,
          cancelLabel,
          resolve: (ok) => {
            setReq(null);
            resolve(ok);
          },
        }),
      ),
    [],
  );
  return (
    <DataProvider
      token="t"
      modules={modulesFor(can)}
      apps={[]}
      notify={notify as never}
      confirm={confirm}
      linkFor={() => null}
    >
      <RecordPanel
        model={request}
        fields={request.fields}
        row={row}
        defaults={{}}
        withheld={[]}
        title="Request"
        onClose={() => {}}
        onSaved={() => {}}
      />
      {req && <ConfirmDialog req={req} />}
    </DataProvider>
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
async function mount(row: Row, can = { make: true, revoke: true }) {
  await act(async () => {
    root.render(<Shell row={row} can={can} />);
  });
  await settle();
  await settle();
}
const click = (el: Element | null | undefined) =>
  act(async () => {
    (el as HTMLElement).click();
  });
const flat = (el: Element | null | undefined) =>
  (el?.textContent ?? "").replace(/\s+/g, " ").trim();

const actionRow = () => container.querySelector(".actionrow")!;
const actionButtons = () => [...actionRow().querySelectorAll<HTMLElement>("button")];
const actionButton = (name: string) => actionButtons().find((b) => flat(b) === name);
const steps = () => [...container.querySelectorAll<HTMLElement>("ol.statusbar button")];
const step = (name: string) => steps().find((b) => flat(b).replace(/ \*$/, "") === name)!;
const note = () => container.querySelector<HTMLElement>("p.sign-by-link");
/** The words a tooltip shows once its control has the focus. */
const tooltipOf = async (el: HTMLElement) => {
  await act(async () => el.focus());
  return document.body.querySelector('[role="tooltip"]')?.textContent ?? "";
};
const disabled = (el: HTMLElement | undefined) => el?.getAttribute("aria-disabled") === "true";

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  history.replaceState(null, "", "/");
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
  nav.ask = null;
  nav.dirty = null;
  notify = vi.fn();
  linkRows = [];
  createAnswer = {
    id: "link-1",
    link: "/sign/r1/token",
    expires_at: "2030-01-01T00:00:00.000000Z",
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

describe("a request about a Document", () => {
  it("has no Sign button, keeps the other actions and the signing-link button", async () => {
    await mount(rows.document);
    const names = actionButtons().map(flat);
    expect(names, "a button to mark it signed by hand").not.toContain("Sign");
    expect(names).toContain("Decline");
    expect(names).toContain("Create signing link");
  });

  it("says in the panel that the signer signs it through the link", async () => {
    await mount(rows.document);
    expect(flat(note())).toContain("The signer signs it through a signing link");
    expect(flat(note())).toContain("It cannot be marked signed by hand.");
  });

  it("greys out the Signed step of the status bar and gives the reason", async () => {
    await mount(rows.document);
    expect(disabled(step("Signed"))).toBe(true);
    expect(await tooltipOf(step("Signed"))).toBe(REASON);
    // The steps it may take are untouched.
    expect(disabled(step("Declined"))).toBe(false);
  });

  it("sends nothing when the greyed-out step is pressed", async () => {
    await mount(rows.document);
    await click(step("Signed"));
    await settle();
    expect(sent("sign_request/sign")).toEqual([]);
    expect(sent("sign_request/update")).toEqual([]);
  });

  it("is the same for a request that was left holding only the hash of a Document", async () => {
    await mount(rows.cleared);
    expect(actionButtons().map(flat)).not.toContain("Sign");
    expect(note()).not.toBeNull();
    expect(disabled(step("Signed"))).toBe(true);
  });

  it("says so while it is a draft too, but not once it is signed", async () => {
    await mount(rows.documentDraft);
    expect(note()).not.toBeNull();
    await mount({ ...rows.documentSigned });
    expect(note()).toBeNull();
  });
});

describe("a request made from a template", () => {
  it("has the Sign button, no note and the Signed step", async () => {
    await mount(rows.template);
    expect(actionButtons().map(flat)).toContain("Sign");
    expect(disabled(actionButton("Sign"))).toBe(false);
    expect(note()).toBeNull();
    expect(disabled(step("Signed"))).toBe(false);
  });

  it("is marked signed by hand when the Sign button is pressed", async () => {
    await mount(rows.template);
    await click(actionButton("Sign"));
    await settle();
    expect(sent("sign_request/sign")).toEqual([
      { url: "/api/sign_request/sign", body: { id: "r1" } },
    ]);
  });

  it("is marked signed by hand when the Signed step is pressed", async () => {
    await mount(rows.template);
    await click(step("Signed"));
    await settle();
    expect(sent("sign_request/sign")).toHaveLength(1);
  });

  it("is still closed by hand when a hash was typed in under the old package", async () => {
    await mount(rows.typedHash);
    expect(actionButtons().map(flat)).toContain("Sign");
    expect(note()).toBeNull();
    expect(disabled(step("Signed"))).toBe(false);
  });
});

describe("Send and Send document in the action row", () => {
  it("greys out Send, with the reason, for a draft about a Document", async () => {
    await mount(rows.documentDraft);
    expect(disabled(actionButton("Send"))).toBe(true);
    expect(disabled(actionButton("Send document"))).toBe(false);
    const send = actionButton("Send")!;
    await act(async () => send.focus());
    expect(document.body.querySelector('[role="tooltip"]')?.textContent).toBe(
      "This request is about a document. Use Send document.",
    );
  });

  it("greys out Send document for a draft made from a template", async () => {
    await mount(rows.templateDraft);
    expect(disabled(actionButton("Send"))).toBe(false);
    expect(disabled(actionButton("Send document"))).toBe(true);
  });
});

describe("who is told that they make the link (W6)", () => {
  it("a person who may make links reads that they create it here", async () => {
    await mount(rows.document, { make: true, revoke: true });
    expect(flat(note())).toContain("that you create here after sending it");
    expect(actionButtons().map(flat)).toContain("Create signing link");
  });

  it("a person who may not make links reads who does, and has no button", async () => {
    await mount(rows.document, { make: false, revoke: false });
    expect(actionButtons().map(flat)).not.toContain("Create signing link");
    expect(flat(note())).not.toContain("you create");
    expect(flat(note())).toContain("a person with the right to make signing links creates");
    expect(flat(note())).toContain("It cannot be marked signed by hand.");
  });
});

describe("revoking a signing link (W4)", () => {
  const link: Row = {
    id: "9f8e7d6c-0000-0000-0000-000000000001",
    request_id: "r1",
    expires_at: "2030-01-01T00:00:00.000000Z",
  };
  const question = () => document.body.querySelector('[role="alertdialog"]');
  const answers = () => [...(question()?.querySelectorAll("button") ?? [])].map(flat);

  it("asks with buttons that are about the link, not about editing", async () => {
    linkRows = [link];
    await mount(rows.document);
    await click([...container.querySelectorAll("button")].find((b) => flat(b) === "Revoke"));
    expect(question(), "no question").not.toBeNull();
    expect(flat(question())).toContain("Revoke this signing link?");
    expect(answers()).toEqual(["Keep it", "Revoke"]);
    expect(answers()).not.toContain("Keep editing");
  });

  it("revokes only on the Revoke button", async () => {
    linkRows = [link];
    await mount(rows.document);
    const revoke = () =>
      [...container.querySelectorAll("button")].find((b) => flat(b) === "Revoke");
    await click(revoke());
    await click([...question()!.querySelectorAll("button")].find((b) => flat(b) === "Keep it"));
    await settle();
    expect(sent("sign_link/update")).toEqual([]);
    await click(revoke());
    await click([...question()!.querySelectorAll("button")].find((b) => flat(b) === "Revoke"));
    await settle();
    expect(sent("sign_link/update")).toHaveLength(1);
    expect((sent("sign_link/update")[0]!.body as { id: string }).id).toBe(link.id);
  });
});

describe("an answer to Create signing link that the page cannot show (W5)", () => {
  // The server made the link before it answered, so a live link exists that nobody has seen: the
  // person must be told to revoke it, not that the server returned no link.
  const refused: [string, unknown][] = [
    ["an address that is not http or https", { id: "l", link: "javascript:alert(1)" }],
    ["a path that is not under /sign/", { id: "l", link: "/elsewhere/abc" }],
    ["a link with a query", { id: "l", link: "/sign/r1/abc?x=1" }],
    ["a link with a fragment", { id: "l", link: "/sign/r1/abc#x" }],
    ["a link with a user name", { id: "l", link: "https://user@example.test/sign/r1/abc" }],
    ["an empty link", { id: "l", link: "" }],
    ["no link at all", { id: "l" }],
  ];
  for (const [what, answer] of refused) {
    it(`says the link was made and is to be revoked when the answer holds ${what}`, async () => {
      createAnswer = answer;
      await mount(rows.document);
      await click(actionButton("Create signing link"));
      await settle();
      await settle();
      expect(document.body.querySelector('[role="dialog"]'), "a dialog opened").toBeNull();
      expect(sent("sign_link/create")).toHaveLength(1);
      expect(notify).toHaveBeenCalledTimes(1);
      const [text, tone] = notify.mock.calls[0]!;
      expect(tone).toBe("error");
      expect(text).toContain("A signing link was made for this request");
      expect(text).toContain("nobody has it");
      expect(text).toContain("Revoke it in the Signing links table and make another one.");
      expect(text).not.toContain("did not return");
    });
  }

  it("shows the link, and says nothing of the kind, when the answer is a link", async () => {
    await mount(rows.document);
    await click(actionButton("Create signing link"));
    await settle();
    await settle();
    expect(document.body.querySelector('[role="dialog"]')).not.toBeNull();
    expect(notify).not.toHaveBeenCalled();
  });
});

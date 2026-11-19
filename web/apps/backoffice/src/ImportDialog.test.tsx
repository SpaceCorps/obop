// @vitest-environment jsdom
// The Import dialog mounted for real (D70 and its amendment for names in a reference column): a
// real DataProvider and Dialog, a fake `fetch` that answers like the server, and jsdom for the DOM.
// A file is chosen the way a person chooses it, and the tests read what was sent and what the page
// says. The pure lookup is tested in importNames.test.ts.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { FieldMeta, ModelMeta, ModuleMeta, Row } from "./api.ts";
import { DataProvider } from "./data.tsx";
import { ImportDialog } from "./ImportDialog.tsx";

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
const model = (name: string, fields: FieldMeta[]): ModelMeta =>
  ({
    name,
    description: "",
    can_create: true,
    can_bulk_create: true,
    can_update: true,
    can_delete: true,
    fields,
    actions: [],
  }) as unknown as ModelMeta;
const lead = model("lead", [
  field("title", { required: true }),
  field("party_id", { kind: "ref", to: "party" }),
]);
const note = model("note", [field("title", { required: true })]);
const party = model("party", [field("name", { required: true })]);
const modules = (): ModuleMeta[] =>
  [{ id: "crm", name: "CRM", models: [lead, note, party] }] as unknown as ModuleMeta[];

const ACME = "0190f0a0-0000-7000-8000-000000000001";
const parties: Row[] = [
  { id: ACME, name: "Acme Example" },
  { id: "0190f0a0-0000-7000-8000-000000000002", name: "Twin" },
  { id: "0190f0a0-0000-7000-8000-000000000003", name: "Twin" },
];

type Sent = { url: string; body: Record<string, unknown> };
let sent: Sent[];
/** What the fake server answers to a bulk_create: the errors of the rows, by row. */
let rowErrors: { row: number; line: number; error: string }[];
let container: HTMLDivElement;
let root: Root;

const json = (value: unknown) =>
  new Response(JSON.stringify({ value }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });

function fakeServer() {
  sent = [];
  rowErrors = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>;
      sent.push({ url: String(url), body });
      if (String(url).includes("/api/party/search")) return json({ rows: parties, total: 3 });
      if (String(url).includes("/bulk_create")) {
        const total = String(body.csv).split(/\r?\n/).filter(Boolean).length - 1;
        const failed = rowErrors.length;
        return json({
          ok: failed === 0,
          total,
          valid: total - failed,
          ...(body.dry_run ? {} : { created: total - failed }),
          failed,
          errors: rowErrors,
        });
      }
      return json({ rows: [], total: 0 });
    }),
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
const text = () => document.body.textContent ?? "";
const bulk = () => sent.filter((s) => s.url.includes("/bulk_create"));
const searches = (m: string) => sent.filter((s) => s.url.includes(`/api/${m}/search`));

async function mount(m: ModelMeta = lead) {
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={modules()}
        apps={[]}
        notify={() => {}}
        confirm={async () => true}
        linkFor={() => null}
      >
        <ImportDialog model={m} title="Leads" onClose={() => {}} />
      </DataProvider>,
    );
  });
}

/** Choose a file as a person does, and wait until the dialog has checked it. */
async function choose(body: string) {
  const input = document.body.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = new File([body], "leads.csv", { type: "text/csv" });
  Object.defineProperty(input, "files", { value: [file], configurable: true });
  await act(async () => {
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  for (let i = 0; i < 40 && !/Rows ready|cannot be imported/.test(text()); i++) await settle();
}

const problemCells = () =>
  [...document.body.querySelectorAll<HTMLElement>("td.cell-main")].map((td) => td.innerHTML);

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
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

describe("the Import dialog with names in a reference column", () => {
  it("says what a reference column takes, only when the model has one", async () => {
    await mount();
    expect(text()).toContain(
      "A column that points to another record (Party\u00a0(party_id)) takes the name of that record",
    );
    expect(text()).not.toContain("References are the id of the record");
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount(note);
    expect(text()).not.toContain("A column that points to another record");
  });

  it("looks a name up once, and the check and the import send the same text", async () => {
    await mount();
    await choose("title,party_id\r\nA, acme EXAMPLE \r\n");
    expect(searches("party")).toHaveLength(1);
    expect(bulk()).toHaveLength(1);
    expect(bulk()[0]!.body.csv).toBe(`title,party_id\r\nA,${ACME}\r\n`);
    expect(bulk()[0]!.body.dry_run).toBe(true);
    // Another mode checks again with the same text, and no new lookup.
    const select = document.body.querySelector<HTMLSelectElement>("select")!;
    await act(async () => {
      select.value = "partial";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    for (let i = 0; i < 10 && bulk().length < 2; i++) await settle();
    await settle();
    const go = [...document.body.querySelectorAll("button")].find((b) =>
      /^Import 1 rows?$/.test((b.textContent ?? "").trim()),
    )!;
    await act(async () => go.click());
    for (let i = 0; i < 20 && bulk().length < 3; i++) await settle();
    expect(bulk().map((b) => b.body.csv)).toEqual([
      `title,party_id\r\nA,${ACME}\r\n`,
      `title,party_id\r\nA,${ACME}\r\n`,
      `title,party_id\r\nA,${ACME}\r\n`,
    ]);
    expect(bulk()[2]!.body.dry_run).toBeUndefined();
    expect(typeof bulk()[2]!.body.idempotency_key).toBe("string");
    expect(searches("party")).toHaveLength(1);
  });

  it("sends a file of ids byte for byte and looks nothing up", async () => {
    await mount();
    const file = `\ufefftitle;party_id\r\n"A; b";${ACME}\r\nB;\r\n`;
    await choose(file);
    // As before this change: `File.text()` decodes UTF-8 and so drops a leading byte order mark
    // (the Encoding standard); every other character is sent as written.
    expect(bulk()[0]!.body.csv).toBe(file.slice(1));
    expect(searches("party")).toHaveLength(0);
  });

  it("shows its own sentence for a name it could not match, and the server's when it is about something else", async () => {
    await mount();
    rowErrors = [
      {
        row: 1,
        line: 2,
        error: "lead.party_id must be the id of a party; there is no party Nobody Example",
      },
      { row: 2, line: 3, error: "lead.title is required" },
    ];
    await choose("title,party_id\nA,Nobody Example\n,Twin\n");
    expect(bulk()[0]!.body.csv).toBe("title,party_id\nA,Nobody Example\n,Twin\n");
    const cells = problemCells();
    // The same thing: the server's words about the missing party are not repeated.
    expect(cells[0]).toBe(
      "No party is named “Nobody Example” (column party_id). Check the spelling, or add it first.",
    );
    // Something else: our sentence, then the server's on a second line.
    expect(cells[1]).toContain(
      "More than one party is named “Twin” (column party_id). Give one of them another name, or write the id of the one you mean: open it and copy the text after “id=” from the address bar.",
    );
    expect(cells[1]).toMatch(/address bar\.<br>.*required/);
  });
});

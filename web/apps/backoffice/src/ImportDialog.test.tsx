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
/** How the fake server answers a search of the parties; a test may hold it or change it. */
let partySearch: () => Promise<Response>;
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
  partySearch = async () => json({ rows: parties, total: 3 });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body ?? "{}")) as Record<string, unknown>;
      sent.push({ url: String(url), body });
      if (String(url).includes("/api/party/search")) return partySearch();
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

const fileInput = () => document.body.querySelector<HTMLInputElement>('input[type="file"]')!;

/** Choose a file as a person does: the browser hands it to the input and says it changed. */
async function put(body: BlobPart, name = "leads.csv") {
  const input = fileInput();
  const file = new File([body], name, { type: "text/csv" });
  Object.defineProperty(input, "files", { value: [file], configurable: true });
  await act(async () => {
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/** Choose a file, and wait until the dialog has checked it. */
async function choose(body: BlobPart, name = "leads.csv") {
  await put(body, name);
  for (let i = 0; i < 40 && !/Rows ready|cannot be imported/.test(text()); i++) await settle();
}

/** The Import button, once a check offers it. */
const importButton = () =>
  [...document.body.querySelectorAll("button")].find((b) =>
    /^Import 1 rows?$/.test((b.textContent ?? "").trim()),
  );

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

  it("a file chosen while the names of another are looked up wins, and the input waits", async () => {
    let release!: () => void;
    const held = new Promise<void>((done) => (release = done));
    partySearch = async () => {
      await held;
      return json({ rows: parties, total: 3 });
    };
    await mount();
    await put("title,party_id\nOld row,Acme Example\n", "first.csv");
    for (let i = 0; i < 10 && searches("party").length === 0; i++) await settle();
    expect(text()).toContain("Looking up the names in your file...");
    // No second file while the names are looked up: the input is disabled with its button.
    expect(fileInput().disabled).toBe(true);
    // A change the browser delivers all the same: the newer file is checked, the older one stops.
    const second = `title,party_id\nNew row,${ACME}\n`;
    await put(second, "second.csv");
    for (let i = 0; i < 20 && !/Rows ready/.test(text()); i++) await settle();
    await act(async () => release());
    for (let i = 0; i < 10; i++) await settle();
    expect(text()).toContain("second.csv");
    expect(bulk().map((b) => b.body.csv)).toEqual([second]);
    await act(async () => importButton()!.click());
    for (let i = 0; i < 20 && bulk().length < 2; i++) await settle();
    expect(bulk()[1]!.body.dry_run).toBeUndefined();
    expect(bulk()[1]!.body.csv).toBe(second);
  });

  it("empties the file input after a pick, so choosing the same file again reads it again", async () => {
    await mount();
    const set = vi.fn();
    Object.defineProperty(fileInput(), "value", { configurable: true, get: () => "", set });
    await choose(`title,party_id\nA,${ACME}\n`);
    expect(set).toHaveBeenCalledWith("");
    expect(bulk()).toHaveLength(1);
  });

  it("does not match a name when the list holds records back, and says it cannot be sure", async () => {
    // More parties than one load gives (the server counts 6,000 and sends one): a name found
    // among the loaded rows may be another record's too.
    partySearch = async () => json({ rows: [parties[0]!], total: 6000 });
    rowErrors = [
      {
        row: 1,
        line: 2,
        error: "lead.party_id must be the id of a party; there is no party Acme Example",
      },
    ];
    await mount();
    await choose("title,party_id\nA,Acme Example\n");
    expect(bulk()[0]!.body.csv).toBe("title,party_id\nA,Acme Example\n");
    expect(problemCells()).toEqual([
      "The import cannot be sure which party “Acme Example” means (column party_id). Write its id instead: open the record and copy the text after “id=” from the address bar.",
    ]);
  });

  it("leaves a column as written when its records cannot be loaded", async () => {
    partySearch = async () =>
      new Response(JSON.stringify({ error: "forbidden" }), {
        status: 403,
        headers: { "content-type": "application/json" },
      });
    rowErrors = [
      {
        row: 1,
        line: 2,
        error: "lead.party_id must be the id of a party; there is no party Acme Example",
      },
    ];
    await mount();
    await choose("title,party_id\nA,Acme Example\n");
    expect(searches("party")).toHaveLength(1);
    expect(bulk()[0]!.body.csv).toBe("title,party_id\nA,Acme Example\n");
    // No sentence of the dialog's: it did not see the records, so the server's words stand.
    expect(problemCells()).toEqual([
      "lead.party_id must be the id of a party; there is no party Acme Example",
    ]);
  });

  it("leaves a column as written when the person may not read the model it points to", async () => {
    // `account` is not among the person's models: no lookup, and the check runs as before.
    const deal = model("deal", [
      field("title", { required: true }),
      field("account_id", { kind: "ref", to: "account" }),
    ]);
    await mount(deal);
    await choose("title,account_id\nA,Main Account\n");
    expect(sent.filter((s) => s.url.includes("/api/account/"))).toHaveLength(0);
    expect(bulk().map((b) => b.body.csv)).toEqual(["title,account_id\nA,Main Account\n"]);
    expect(text()).toContain("Rows ready");
  });

  it("refuses a file that the ids make larger than 1 MiB, and sends nothing", async () => {
    // 1000 rows of a long title and a name of 12 characters: 1,035,015 bytes as chosen, and
    // 24,000 more once each name is its id of 36 characters.
    const body = `title,party_id\n${`${"t".repeat(1021)},Acme Example\n`.repeat(1000)}`;
    expect(new TextEncoder().encode(body).length).toBe(1_035_015);
    await mount();
    await choose(body);
    expect(searches("party")).toHaveLength(1);
    expect(text()).toContain(
      "With the names looked up, the file is larger than 1 MiB. Split it into smaller files.",
    );
    expect(bulk()).toHaveLength(0);
  });

  it("sends a file that is over 1 MiB only once read, as chosen, and leaves the words to the server", async () => {
    // Files of about 400 KB, most of their bytes not UTF-8: each becomes U+FFFD (three bytes) when
    // read, so the text is over 1 MiB though no name made it so. The server's sentence stands.
    const enc = new TextEncoder();
    const bad = (n: number) => [...new Uint8Array(n).fill(0xff)];
    // One cell of 400,000 such bytes (over the cell limit once read)...
    const oneCell = new Uint8Array([
      ...enc.encode("title,party_id\nA,Nobody\nB,"),
      ...bad(400_000),
      10,
    ]);
    // ...and 999 cells of 400 (each under it; only the whole text is over).
    const row = [...enc.encode("B,"), ...bad(400), 10];
    const manyCells = new Uint8Array([
      ...enc.encode("title,party_id\nA,Nobody\n"),
      ...Array.from({ length: 999 }, () => row).flat(),
    ]);
    expect([oneCell.length, manyCells.length]).toEqual([400_027, 402_621]);
    await mount();
    await choose(oneCell);
    expect(bulk()).toHaveLength(1);
    await choose(manyCells);
    expect(bulk()).toHaveLength(2);
    expect(text()).not.toContain("With the names looked up");
    expect(searches("party")).toHaveLength(0);
  });
});

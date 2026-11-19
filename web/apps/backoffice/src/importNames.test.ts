import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta, ModelMeta, ModuleMeta, Row } from "./api.ts";
import fixtures from "./csvRead.fixtures.json";
import { recordLabelOf, type ModelInfo } from "./data.tsx";
import {
  decide,
  fixedLabel,
  lookupColumns,
  overLimit,
  readSheet,
  resolveNames,
  sameRefusal,
  shortName,
  splice,
  targetOf,
  titleReads,
  type Look,
  type Target,
} from "./importNames.ts";
import { titleOf } from "./presentation.ts";

// D70 amendment: names in a reference column of the Import dialog. The CSV cases are shared with
// the server: crates/kernel/obop-commands/src/csv.rs reads csvRead.fixtures.json too, so the browser
// never reads a cell into another column than the server does.

const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const model = (name: string, fields: FieldMeta[], title?: string): ModelMeta =>
  ({
    name,
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields,
    actions: [],
    ...(title ? { ui: { title } } : {}),
  }) as unknown as ModelMeta;
const ref = (name: string, to: string, extra: Partial<FieldMeta> = {}) =>
  field(name, { kind: "ref", to, ...extra });

const party = model("party", [
  field("name", { required: true }),
  field("active", { kind: "bool" }),
]);
const lead = model("lead", [
  field("title", { required: true }),
  ref("party_id", "party"),
  ref("account_id", "account"),
  field("stage", { kind: "select", options: ["new", "won"] }),
]);
const account = model("account", [field("code"), field("name", { required: true })]);

/** What the store holds: models and their loaded rows. */
function world(models: ModelMeta[], rows: Record<string, Row[]>): Look {
  const byName = new Map(models.map((m) => [m.name, m]));
  return { model: (n) => byName.get(n), rows: (n) => rows[n] };
}

/** The label of a record as data.tsx makes it (labelAt): titleOf, refs followed to depth 3. */
function labeller(look: Look, models: ModelMeta[]) {
  const infos = new Map<string, ModelInfo>(
    models.map((m) => [m.name, { model: m, module: { id: "t" } as ModuleMeta }]),
  );
  const at = (m: string, row: Row, depth: number): string => {
    const meta = look.model(m);
    if (!meta || depth > 3) return recordLabelOf(infos, m, row);
    return titleOf(meta, row, {
      refText: (to, id) => {
        const r = look.rows(to)?.find((x) => x.id === id);
        return r ? at(to, r, depth + 1) : id.slice(0, 8);
      },
      rowOf: (to, id) => look.rows(to)?.find((x) => x.id === id),
      modelOf: (to) => look.model(to),
      moduleId: "t",
    });
  };
  return (m: string) => (row: Row) => at(m, row, 0);
}

/** The targets of the dialog: every column's model, complete unless said otherwise. */
function targets(look: Look, models: ModelMeta[], complete = true): Map<string, Target | null> {
  const label = labeller(look, models);
  const out = new Map<string, Target | null>();
  for (const m of models)
    if (look.rows(m.name)) out.set(m.name, targetOf(m, look, label(m.name), complete));
  return out;
}

/** Read, look up and replace, as the dialog does. */
function run(text: string, look: Look, models: ModelMeta[], complete = true) {
  const sheet = readSheet(text);
  const columns = lookupColumns(sheet, lead);
  if (!sheet || columns.length === 0) return { text, problems: new Map() };
  return resolveNames(text, sheet, columns, targets(look, models, complete));
}

const ACME = "0190f0a0-0000-7000-8000-000000000001";
const BETA = "0190f0a0-0000-7000-8000-000000000002";
const OLD = "0190f0a0-0000-7000-8000-000000000003";
const parties: Row[] = [
  { id: ACME, name: "Acme Example" },
  { id: BETA, name: "Beta, Ltd" },
  { id: OLD, name: "Old Example", active: false },
];
const base = () => world([lead, party, account], { party: parties, account: [] });

describe("readSheet reads the file as csv.rs does (csvRead.fixtures.json)", () => {
  for (const f of fixtures) {
    it(f.name, () => {
      const sheet = readSheet(f.text);
      if ("refused" in f.expect) {
        expect(sheet).toBeNull();
        return;
      }
      expect(sheet).not.toBeNull();
      expect(sheet!.delimiter).toBe(f.expect.delimiter);
      expect(sheet!.headers).toEqual(f.expect.headers);
      expect(sheet!.records.map((r) => r.cells.map((c) => c.value))).toEqual(f.expect.rows);
      expect(sheet!.records.map((r) => r.line)).toEqual(f.expect.lines);
      expect(sheet!.records.map((r) => r.row)).toEqual(f.expect.rows.map((_, i) => i + 1));
      // Every span slices back to the cell as written: quoted, or the value itself.
      for (const r of sheet!.records)
        for (const c of r.cells) {
          const raw = f.text.slice(c.start, c.end);
          expect(raw.startsWith('"') ? raw.slice(1, -1).replaceAll('""', '"') : raw).toBe(c.value);
        }
    });
  }
});

describe("importNames", () => {
  it("splice changes only the named spans", () => {
    const text = '\ufefftitle;party_id\r\n\r\nA;"Acme ""Example"""\rB;x\n';
    const sheet = readSheet(text)!;
    const cell = sheet.records[0]!.cells[1]!;
    expect(cell.value).toBe('Acme "Example"');
    expect(splice(text, [{ start: cell.start, end: cell.end, text: ACME }])).toBe(
      `\ufefftitle;party_id\r\n\r\nA;${ACME}\rB;x\n`,
    );
    const b = sheet.records[1]!.cells[1]!;
    expect(
      splice(text, [
        { start: b.start, end: b.end, text: BETA },
        { start: cell.start, end: cell.end, text: ACME },
      ]),
    ).toBe(`\ufefftitle;party_id\r\n\r\nA;${ACME}\rB;${BETA}\n`);
  });

  it("nothing to look up returns the same string", () => {
    const look = base();
    const same = (text: string, l = look, models = [lead, party, account]) =>
      expect(Object.is(run(text, l, models).text, text)).toBe(true);
    // Ids only, and empty cells.
    same(`title,party_id\r\nA,${ACME}\r\nB,\r\nC, ${BETA} \r\n`);
    // No reference column.
    same("title,stage\nA,Acme Example\n");
    expect(lookupColumns(readSheet("title,stage\nA,Acme Example\n"), lead)).toEqual([]);
    // A file the server refuses for its structure.
    expect(readSheet('title,party_id\nA,"Acme\n')).toBeNull();
    same('title,party_id\nA,"Acme\n');
    // A model the person may not read: no target, the column stays as written.
    const text = "title,party_id\nA,Acme Example\n";
    expect(Object.is(run(text, world([lead], {}), [lead]).text, text)).toBe(true);
    expect(run(text, world([lead], {}), [lead]).problems.size).toBe(0);
  });

  it("a name becomes the id and rows keep their lines", () => {
    const text =
      'title,party_id\r\n"Two\nlines",  acme   EXAMPLE \r\nB,"Beta, Ltd"\r\n\r\nC,acme example\r\n';
    const out = run(text, base(), [lead, party, account]);
    expect(out.problems.size).toBe(0);
    expect(out.text).toBe(
      `title,party_id\r\n"Two\nlines",${ACME}\r\nB,${BETA}\r\n\r\nC,${ACME}\r\n`,
    );
    const before = readSheet(text)!;
    const after = readSheet(out.text)!;
    expect(after.records.map((r) => [r.row, r.line])).toEqual(
      before.records.map((r) => [r.row, r.line]),
    );
    expect(after.records.map((r) => r.line)).toEqual([2, 4, 6]);
  });

  it("an id wins over a name", () => {
    // A record whose name is another record's id: the id wins, as yesterday.
    const look = world([lead, party, account], {
      party: [
        { id: "p-1", name: "Acme Example" },
        { id: "p-2", name: "p-1" },
        { id: "p-3", name: "Archived", active: false },
      ],
    });
    const text = `title,party_id\nA,p-1\nB,p-3\nC,${OLD}\n`;
    const out = run(text, look, [lead, party, account]);
    expect(out.text).toBe(text);
    expect(out.problems.size).toBe(0);
  });

  it("a cell with a line break is never touched", () => {
    const text = 'title,party_id\nA,"Acme\nExample"\n';
    const out = run(text, base(), [lead, party, account]);
    expect(Object.is(out.text, text)).toBe(true);
    expect(out.problems.size).toBe(0);
  });

  it("two records of one name, no record of that name, only an archived record", () => {
    const look = world([lead, party, account], {
      party: [...parties, { id: "dup-1", name: "Twin" }, { id: "dup-2", name: " twin " }],
    });
    const text = "title,party_id\nA,Twin\nB,Nobody Example\nC,old example\n";
    const out = run(text, look, [lead, party, account]);
    expect(out.text).toBe(text);
    expect([...out.problems.entries()].map(([row, p]) => [row, p.kind, p.header, p.text])).toEqual([
      [1, "several", "party_id", "Twin"],
      [2, "none", "party_id", "Nobody Example"],
      [3, "archived", "party_id", "old example"],
    ]);
  });

  it("a list that may be incomplete never matches", () => {
    const text = "title,party_id\nA,Acme Example\nB,Nobody Example\n";
    const out = run(text, base(), [lead, party, account], false);
    expect(out.text).toBe(text);
    expect([...out.problems.values()].map((p) => p.kind)).toEqual(["unsure", "unsure"]);
  });

  it("a label that changes with the language is never matched", () => {
    const kinds = model("kind", [
      field("label", { required: true, shows_option_of: "kind.code" }),
      field("code", { kind: "select", options: ["a", "b"] }),
    ]);
    const byState = model(
      "deal",
      [field("stage", { kind: "select", options: ["new", "won"] })],
      "{stage}",
    );
    const byDate = model("visit", [field("on", { kind: "date" })], "{on}");
    const invoice = model("invoice", [
      field("number", { sequence: true, kind: "number" }),
      field("state", { kind: "select", options: ["draft", "posted"], governed: true }),
      ref("party_id", "party"),
    ]);
    const vendor = model("vendor", [ref("party_id", "party")], "{party_id}");
    const look = world([party, kinds, byState, byDate, invoice, vendor], {
      party: parties,
      kind: [{ id: "k1", label: "a", code: "a" }],
      deal: [{ id: "d1", stage: "won" }],
      visit: [{ id: "v1", on: "2026-11-19" }],
      invoice: [{ id: "i1", state: "draft", party_id: ACME }],
      vendor: [{ id: "w1", party_id: "missing-party" }],
    });
    expect(fixedLabel(kinds, look.rows("kind")![0]!, look)).toBe(false);
    expect(fixedLabel(byState, look.rows("deal")![0]!, look)).toBe(false);
    expect(fixedLabel(byDate, look.rows("visit")![0]!, look)).toBe(false);
    expect(fixedLabel(invoice, look.rows("invoice")![0]!, look)).toBe(false);
    expect(fixedLabel(vendor, look.rows("vendor")![0]!, look)).toBe(false);
    // A name equal to the label such a row shows today is not matched; one no row is shown under
    // is "none", also when the model has rows of a changing label (R1).
    const label = labeller(look, [party, kinds, byState, byDate, invoice, vendor]);
    const deals = targetOf(byState, look, label("deal"), true);
    const shown = label("deal")(look.rows("deal")![0]!);
    expect(shown).not.toBe("");
    expect(decide(deals, shown)).toEqual({ kind: "unsure" });
    expect(decide(deals, "Lost")).toEqual({ kind: "none" });
    const vendors = targetOf(vendor, look, label("vendor"), true);
    expect(decide(vendors, "missing-")).toEqual({ kind: "unsure" });
    expect(decide(vendors, "Acme Example")).toEqual({ kind: "none" });
  });

  it("a composed title matches", () => {
    const vendor = model("vendor", [ref("party_id", "party")], "{party_id}");
    const folder = model("folder", [field("name", { required: true }), ref("parent_id", "folder")]);
    const models = [lead, party, account, vendor, folder];
    const look = world(models, {
      party: parties,
      account: [
        { id: "a1", code: "1400", name: "Receivables" },
        { id: "a2", code: "1500", name: "" },
      ],
      vendor: [{ id: "w1", party_id: ACME }],
      folder: [
        { id: "f1", name: "Contracts" },
        { id: "f2", name: "Rental", parent_id: "f1" },
      ],
    });
    const label = labeller(look, models);
    const t = (m: ModelMeta) => targetOf(m, look, label(m.name), true);
    expect(decide(t(account), "1400 receivables")).toEqual({ kind: "one", id: "a1" });
    expect(fixedLabel(account, look.rows("account")![1]!, look)).toBe(false);
    expect(decide(t(vendor), "ACME EXAMPLE")).toEqual({ kind: "one", id: "w1" });
    expect(decide(t(folder), "Contracts / Rental")).toEqual({ kind: "one", id: "f2" });
    expect(decide(t(folder), "Rental")).toEqual({ kind: "none" });
    // Through the whole file: a {code} {name} reference is replaced.
    const out = run("title,account_id\nA,1400 Receivables\n", look, models);
    expect(out.text).toBe("title,account_id\nA,a1\n");
  });

  it("titleReads stops at depth 3, also in a cycle", () => {
    const a = model("m_a", [ref("b_id", "m_b")], "{b_id}");
    const b = model("m_b", [ref("c_id", "m_c")], "{c_id}");
    const c = model("m_c", [ref("d_id", "m_d")], "{d_id}");
    const d = model("m_d", [ref("e_id", "m_e")], "{e_id}");
    const e = model("m_e", [ref("f_id", "m_f")], "{f_id}");
    const look = world([a, b, c, d, e], {});
    expect(titleReads(a, look)).toEqual(["m_b", "m_c", "m_d", "m_e"]);
    const x = model("m_x", [ref("y_id", "m_y")], "{y_id}");
    const y = model("m_y", [ref("x_id", "m_x"), ref("z_id", "m_z")], "{x_id} {z_id}");
    const z = model("m_z", [field("name", { required: true })]);
    expect(titleReads(x, world([x, y, z], {}))).toEqual(["m_y", "m_z"]);
    expect(titleReads(party, world([party], {}))).toEqual([]);
  });

  it("a withheld title field makes the row not fixed", () => {
    const look = world([party], { party: [{ id: "p1" }, { id: "p2", name: "Seen" }] });
    expect(fixedLabel(party, look.rows("party")![0]!, look)).toBe(false);
    expect(fixedLabel(party, look.rows("party")![1]!, look)).toBe(true);
  });

  it("the first problem of a row wins, in column order", () => {
    const text = "title,account_id,party_id\nA,Nowhere,Twin Example\n";
    const look = world([lead, party, account], {
      party: [...parties, { id: "t1", name: "Twin Example" }, { id: "t2", name: "Twin Example" }],
      account: [],
    });
    const out = run(text, look, [lead, party, account]);
    const p = out.problems.get(1)!;
    expect([p.kind, p.header, p.to, p.text]).toEqual(["none", "account_id", "account", "Nowhere"]);
    // Only the server's refusal of that very cell is about the same thing (R3). Its refusal of the
    // other cell (the server names the first failing field in the model's order) is shown too.
    expect(
      sameRefusal(p, "lead.account_id must be an account id; there is no account Nowhere"),
    ).toBe(true);
    expect(sameRefusal(p, "lead.party_id must be a party id; there is no party Twin Example")).toBe(
      false,
    );
    expect(sameRefusal(p, "lead.title is required")).toBe(false);
    expect(sameRefusal(p, "lead.party_id must be a party id; there is no party Nowhere")).toBe(
      false,
    );
  });

  it("shortName cuts at 40 characters", () => {
    expect(shortName("  Acme Example  ")).toBe("Acme Example");
    const long = "x".repeat(41);
    expect(shortName(long)).toBe(`${"x".repeat(39)}…`);
    expect([...shortName(long)].length).toBe(40);
    expect(shortName("x".repeat(40))).toBe("x".repeat(40));
  });

  it("a file over 1 MiB once the names are replaced is refused", () => {
    expect(overLimit("x".repeat(1_048_576))).toBe(false);
    expect(overLimit("x".repeat(1_048_577))).toBe(true);
    expect(overLimit("å".repeat(524_289))).toBe(true);
  });
});

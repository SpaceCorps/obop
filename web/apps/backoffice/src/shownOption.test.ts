import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import type { FieldMeta, ModelMeta, ModuleMeta } from "./api.ts";
import { recordLabelOf, type ModelInfo } from "./data.tsx";
import { plainText, shownLabel } from "./format.ts";
import { resolveShownOptions } from "./shownOption.ts";

async function use(lang: Lang, messages: Record<string, string> = {}) {
  provideCatalogue(lang, messages);
  await setLanguage(lang, false);
}
afterEach(() => use("en"));

const field = (
  name: string,
  kind: FieldMeta["kind"],
  extra: Partial<FieldMeta> = {},
): FieldMeta => ({
  name,
  kind,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});

const model = (name: string, fields: FieldMeta[]): ModelMeta => ({
  name,
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields,
  actions: [],
});

const module = (id: string, models: ModelMeta[]): ModuleMeta => ({ id, name: id, models });

/** A case file with a status select, and a history whose text field holds the status or a free note. */
function desk(entryExtra: Partial<FieldMeta> = {}, statusExtra: Partial<FieldMeta> = {}) {
  const status = field("status", "select", {
    options: ["draft", "pagar", "klar"],
    option_labels: { pagar: { sv: "Pågår", de: "In Arbeit" }, klar: { sv: "Klar" } },
    ...statusExtra,
  });
  const text = field("text", "longtext", {
    shows_option_of: "desk_case.status",
    shows_option_when: 'kind == "status"',
    ...entryExtra,
  });
  const kind = field("kind", "text");
  const modules = [
    module("desk", [model("desk_case", [status]), model("desk_entry", [kind, text])]),
  ];
  resolveShownOptions(modules);
  return { text, status, kind };
}

describe("a text field that shows the label of an option", () => {
  it("shows a stored key as the label in the language of the person", async () => {
    const { text } = desk();
    await use("sv");
    expect(shownLabel(text, "pagar", { kind: "status" })).toBe("Pågår");
    expect(shownLabel(text, "klar", { kind: "status" })).toBe("Klar");
    await use("de");
    expect(shownLabel(text, "pagar", { kind: "status" })).toBe("In Arbeit");
  });

  it("falls back as a select does: the generic table, then the humanised key", async () => {
    const { text } = desk();
    await use("sv");
    // `draft` has no label of the package: the generic words, which Swedish does not have here either.
    expect(shownLabel(text, "draft", { kind: "status" })).toBe("Draft");
    // German has no label for `klar`.
    await use("de");
    expect(shownLabel(text, "klar", { kind: "status" })).toBe("Klar");
  });

  it("leaves a free note alone while the condition is false", async () => {
    const { text } = desk();
    await use("sv");
    expect(shownLabel(text, "klar", { kind: "note" })).toBeUndefined();
    expect(shownLabel(text, "klar", {})).toBeUndefined();
    // No record to ask: the condition cannot hold.
    expect(shownLabel(text, "klar")).toBeUndefined();
  });

  it("shows a value that is not an option as stored", async () => {
    const { text } = desk();
    await use("sv");
    expect(shownLabel(text, "a free note", { kind: "status" })).toBeUndefined();
    expect(shownLabel(text, "Pagar", { kind: "status" })).toBeUndefined();
    expect(shownLabel(text, "", { kind: "status" })).toBeUndefined();
    expect(shownLabel(text, null, { kind: "status" })).toBeUndefined();
    expect(shownLabel(text, 5, { kind: "status" })).toBeUndefined();
  });

  it("has no condition to ask when none is declared", async () => {
    const { text } = desk({ shows_option_when: undefined });
    await use("sv");
    expect(shownLabel(text, "pagar")).toBe("Pågår");
    expect(shownLabel(text, "pagar", { kind: "note" })).toBe("Pågår");
  });

  it("changes nothing that is not drawn: the sort and export text keeps the stored text", async () => {
    const { text } = desk();
    await use("sv");
    expect(plainText(text, "pagar", () => "")).toBe("pagar");
  });

  it("shows a field that names nothing, or a field that is not text, as it is", async () => {
    const { kind } = desk();
    await use("sv");
    expect(shownLabel(kind, "pagar", { kind: "status" })).toBeUndefined();
  });

  it("shows the stored text when the select is not among the fields the person can see", async () => {
    const text = field("text", "longtext", { shows_option_of: "desk_case.status" });
    resolveShownOptions([module("desk", [model("desk_entry", [text])])]);
    await use("sv");
    expect(shownLabel(text, "pagar", {})).toBeUndefined();
  });

  it("shows the stored text when the named field is not a select", async () => {
    const note = field("note", "text");
    const text = field("text", "text", { shows_option_of: "desk_case.note" });
    resolveShownOptions([
      module("desk", [model("desk_case", [note]), model("desk_entry", [text])]),
    ]);
    await use("sv");
    expect(shownLabel(text, "pagar", {})).toBeUndefined();
  });

  it("never relabels when the browser cannot read the condition", async () => {
    const { text } = desk({ shows_option_when: "n + 1 > 2" });
    await use("sv");
    expect(shownLabel(text, "pagar", { n: 5 })).toBeUndefined();
  });

  it("follows the select of another module", async () => {
    const status = field("status", "select", {
      options: ["pagar"],
      option_labels: { pagar: { sv: "Pågår" } },
    });
    const text = field("text", "text", { shows_option_of: "base_case.status" });
    resolveShownOptions([
      module("base", [model("base_case", [status])]),
      module("desk", [model("desk_entry", [text])]),
    ]);
    await use("sv");
    expect(shownLabel(text, "pagar", {})).toBe("Pågår");
  });
});

describe("the label of a record named by such a field", () => {
  /** desk_entry names itself by its text field, which shows an option; desk_task refers to it. */
  function world() {
    const { text, status } = desk();
    const kind = field("kind", "text");
    // The text comes first, so it is the label field of the model (the first text field).
    const entry = model("desk_entry", [text, kind]);
    const caseModel = model("desk_case", [status]);
    const entryRef = field("entry_id", "ref", { to: "desk_entry" });
    const task = model("desk_task", [entryRef]);
    const modules = [module("desk", [caseModel, entry, task])];
    resolveShownOptions(modules);
    const models = new Map<string, ModelInfo>();
    for (const m of modules) for (const x of m.models) models.set(x.name, { model: x, module: m });
    return { models, entry, entryRef, text };
  }

  it("is in the language active when it is asked, not when it was first made", async () => {
    const { models } = world();
    const row = { id: "0123456789", kind: "status", text: "pagar" };
    await use("sv");
    expect(recordLabelOf(models, "desk_entry", row)).toBe("Pågår");
    // Nothing is kept from the first answer: a memo that holds this label must be made again
    // when the language changes (the data context carries `lang` for that).
    await use("de");
    expect(recordLabelOf(models, "desk_entry", row)).toBe("In Arbeit");
    await use("en");
    expect(recordLabelOf(models, "desk_entry", row)).toBe("Pagar");
  });

  it("is the stored text where the condition is false, and the short id where there is none", async () => {
    const { models } = world();
    await use("sv");
    expect(
      recordLabelOf(models, "desk_entry", { id: "0123456789", kind: "note", text: "klar" }),
    ).toBe("klar");
    expect(recordLabelOf(models, "desk_entry", { id: "0123456789", kind: "status" })).toBe(
      "01234567",
    );
  });

  it("names the record by its first required text field, else a numbered one, else the first text or long text", async () => {
    const required = field("title", "text", { required: true });
    const first = field("note", "longtext", {
      shows_option_of: "desk_case.status",
    });
    const status = field("status", "select", {
      options: ["pagar"],
      option_labels: { pagar: { sv: "Pågår" } },
    });
    const modules = [
      module("desk", [model("desk_case", [status]), model("desk_item", [first, required])]),
    ];
    resolveShownOptions(modules);
    const models = new Map<string, ModelInfo>();
    for (const m of modules) for (const x of m.models) models.set(x.name, { model: x, module: m });
    await use("sv");
    // The required text field names the record, although the long text field comes first.
    expect(
      recordLabelOf(models, "desk_item", { id: "abcdefghij", note: "pagar", title: "pagar" }),
    ).toBe("pagar");
    // Without it, the first text or long text field does, and it shows its option.
    const open = new Map(models);
    open.set("desk_item", { model: model("desk_item", [first]), module: modules[0]! });
    expect(recordLabelOf(open, "desk_item", { id: "abcdefghij", note: "pagar" })).toBe("Pågår");
  });

  it("is what a ref column shows, sorts and exports, like any record name; the text field itself exports what is stored", async () => {
    const { models, entryRef, text } = world();
    const row = { id: "e1", kind: "status", text: "pagar" };
    const refText = (m: string, id: string) =>
      id === "e1" ? recordLabelOf(models, m, row) : id.slice(0, 8);
    await use("sv");
    expect(plainText(entryRef, "e1", refText)).toBe("Pågår");
    expect(plainText(text, "pagar", refText)).toBe("pagar");
    await use("en");
    expect(plainText(entryRef, "e1", refText)).toBe("Pagar");
    expect(plainText(text, "pagar", refText)).toBe("pagar");
  });
});

// D70 amendment (PROPOSED): a reference column of a CSV file may hold the other record's name as the
// lists show it, or its id as before. The Import dialog reads the file the way the server will
// (`readSheet`, held to crates/kernel/obop-commands/src/csv.rs by csvRead.fixtures.json), looks the
// names up among the records the person may read, and replaces each name it found exactly once with
// that record's id. Every other character of the file stays as written, so the server's Row and
// Line numbers are the person's; a file with nothing to look up is sent as the very same string.
// A name it cannot be sure of is never guessed: it stays as written, the server refuses that row,
// and the dialog says why in its own words. Pure, so it is tested without a server.
import type { FieldMeta, ModelMeta, Row } from "./api.ts";
import { isEmpty } from "./format.ts";
import { nameKey } from "./inputs/refFind.ts";
import { stateFieldOf, titleFieldsOf, templateFields } from "./presentation.ts";

// ---- reading the file as the server does --------------------------------------------------------

/** One cell: where it starts and ends in the text (its quotes included) and its value. */
export type Span = { start: number; end: number; value: string };
export type Sheet = {
  delimiter: "," | ";";
  headers: string[];
  /** The kept data records: `row` counts from 1 among them, `line` is where each starts in the file. */
  records: { row: number; line: number; cells: Span[] }[];
};

/**
 * Rust's `str::trim`: the Unicode White_Space characters. Not JavaScript's `trim`, which also takes
 * U+FEFF and leaves U+0085, so a header or a cell would be read differently from the server.
 */
const RUST_SPACE =
  "\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000";
const RUST_TRIM = new RegExp(`^[${RUST_SPACE}]+|[${RUST_SPACE}]+$`, "g");
export const rustTrim = (s: string): string => s.replace(RUST_TRIM, "");

/** `;` when the first record has more semicolons than commas outside quotes, else `,` (csv.rs). */
function delimiterOf(text: string, from: number): "," | ";" {
  let commas = 0;
  let semis = 0;
  let quoted = false;
  for (let i = from; i < text.length; i++) {
    const c = text[i];
    if (c === '"') quoted = !quoted;
    else if ((c === "\n" || c === "\r") && !quoted) break;
    else if (c === "," && !quoted) commas++;
    else if (c === ";" && !quoted) semis++;
  }
  return semis > commas ? ";" : ",";
}

/** The limits of csv.rs: most data rows, most columns, longest cell in bytes of UTF-8. */
const MAX_ROWS = 1000;
const MAX_COLUMNS = 100;
const MAX_CELL_BYTES = 65_536;

/** The length of `s` in bytes of UTF-8, as the server counts it. */
function utf8Length(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff && (s.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}

/**
 * The file as `csv::parse` reads it, with where each cell is in the text; `null` where the server
 * refuses the file, for its structure or its limits (size, rows, columns, cell length). Then the
 * text is sent unchanged and the server says why: no name is looked up in a file it refuses.
 */
export function readSheet(text: string): Sheet | null {
  if (overLimit(text)) return null;
  const from = text.startsWith("\ufeff") ? 1 : 0;
  if (text.includes("\0")) return null;
  const delim = delimiterOf(text, from);
  const records: { line: number; cells: Span[] }[] = [];
  let line = 1;
  let recordLine = 1;
  let cells: Span[] = [];
  let cell = "";
  let cellStart = from;
  let atStart = true;
  let seenAny = false;
  const endCell = (end: number) => {
    cells.push({ start: cellStart, end, value: cell });
    cell = "";
    atStart = true;
  };
  const endRecord = (end: number) => {
    endCell(end);
    // A blank line is one empty cell: skip it.
    if (!(cells.length === 1 && cells[0]!.value === "")) records.push({ line: recordLine, cells });
    cells = [];
    seenAny = false;
  };
  let p = from;
  while (p < text.length) {
    const c = text[p]!;
    if (!seenAny) {
      recordLine = line;
      seenAny = true;
    }
    if (c === '"' && atStart) {
      // A quoted cell.
      atStart = false;
      p++;
      for (;;) {
        if (p >= text.length) return null; // a quote opened and never closed
        const ch = text[p]!;
        if (ch === '"') {
          if (text[p + 1] === '"') {
            cell += '"';
            p += 2;
            continue;
          }
          p++;
          break;
        }
        if (ch === "\n") line++;
        cell += ch;
        p++;
      }
      const next = text[p];
      if (next !== undefined && next !== "\n" && next !== "\r" && next !== delim) return null;
    } else if (c === '"') {
      return null; // a quote inside an unquoted cell
    } else if (c === delim) {
      endCell(p);
      p++;
      cellStart = p;
    } else if (c === "\r") {
      const end = p;
      p++;
      if (text[p] === "\n") p++;
      endRecord(end);
      line++;
      cellStart = p;
    } else if (c === "\n") {
      endRecord(p);
      p++;
      line++;
      cellStart = p;
    } else {
      atStart = false;
      cell += c;
      p++;
    }
  }
  if (seenAny) {
    // The last record has no line break after it.
    cells.push({ start: cellStart, end: text.length, value: cell });
    if (!(cells.length === 1 && cells[0]!.value === "")) records.push({ line: recordLine, cells });
  }
  const [head, ...data] = records;
  if (!head) return null; // empty: the first line must name the fields
  if (head.cells.length > MAX_COLUMNS || data.length > MAX_ROWS) return null;
  // Only a cell longer than a third of the limit in UTF-16 units can be over it in bytes.
  for (const r of records)
    for (const c of r.cells)
      if (c.value.length > MAX_CELL_BYTES / 3 && utf8Length(c.value) > MAX_CELL_BYTES) return null;
  const headers = head.cells.map((s) => rustTrim(s.value));
  for (let i = 0; i < headers.length; i++)
    if (headers[i] === "" || headers.indexOf(headers[i]!) < i) return null;
  if (data.some((r) => r.cells.length !== headers.length)) return null;
  return {
    delimiter: delim,
    headers,
    records: data.map((r, i) => ({ row: i + 1, line: r.line, cells: r.cells })),
  };
}

/** `text` with each span replaced; the very same string when there is nothing to replace. */
export function splice(
  text: string,
  edits: readonly { start: number; end: number; text: string }[],
): string {
  if (edits.length === 0) return text;
  const sorted = [...edits].sort((a, b) => a.start - b.start);
  let out = "";
  let at = 0;
  for (const e of sorted) {
    out += text.slice(at, e.start) + e.text;
    at = e.end;
  }
  return out + text.slice(at);
}

// ---- what needs a lookup ----------------------------------------------------------------------

/** The form of every id the server makes (`Uuid::now_v7`). */
export const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * Whether a cell of a reference column is a name to look up: not empty, no line break (replacing it
 * would move every later Line of the server's report), and not written as an id.
 */
export function needsLookup(value: string): boolean {
  const v = rustTrim(value);
  return v !== "" && !/[\r\n]/.test(value) && !UUID.test(v);
}

/** A name as a person means it: no case, no space at either end, one space between words. */
export function importKey(s: string): string {
  return nameKey(s.normalize("NFC"));
}

export type RefColumn = { index: number; header: string; field: FieldMeta; to: string };

/** The columns of the file that are reference fields of `model` a file may fill. */
export function refColumns(sheet: Sheet, model: ModelMeta): RefColumn[] {
  const out: RefColumn[] = [];
  sheet.headers.forEach((header, index) => {
    const field = model.fields.find((f) => f.name === header);
    if (field && field.kind === "ref" && field.to && !field.computed && !field.sequence)
      out.push({ index, header, field, to: field.to });
  });
  return out;
}

/** The reference columns with at least one cell to look up; none: the file is sent as it is. */
export function lookupColumns(sheet: Sheet | null, model: ModelMeta): RefColumn[] {
  if (!sheet) return [];
  return refColumns(sheet, model).filter((c) =>
    sheet.records.some((r) => needsLookup(r.cells[c.index]?.value ?? "")),
  );
}

/** The most the server takes in one call, in bytes of UTF-8 (csv.rs `MAX_BYTES`). */
export const MAX_BYTES = 1_048_576;

/** An id is longer than most names, so a file can grow past the limit once they are replaced. */
export const overLimit = (text: string): boolean =>
  new TextEncoder().encode(text).length > MAX_BYTES;

// ---- which labels can be matched ----------------------------------------------------------------

export type Look = {
  model: (name: string) => ModelMeta | undefined;
  rows: (name: string) => readonly Row[] | undefined;
};

const fieldOf = (model: ModelMeta, name: string) => model.fields.find((f) => f.name === name);

/** The models whose rows a record's label of `model` reads (its title's refs), to depth 3. */
export function titleReads(model: ModelMeta, look: Look, depth = 0): string[] {
  const out: string[] = [];
  const visit = (m: ModelMeta, d: number) => {
    if (d > 3) return;
    const hint = m.ui?.title;
    const names = hint ? templateFields(hint) : titleFieldsOf(m);
    for (const name of names) {
      const f = fieldOf(m, name);
      if (!f || f.kind !== "ref" || !f.to || f.to === model.name || out.includes(f.to)) continue;
      out.push(f.to);
      const next = look.model(f.to);
      if (next) visit(next, d + 1);
    }
  };
  visit(model, depth);
  return out;
}

/** How a placeholder of a title stands in a row: shown, empty, or not something we can be sure of. */
type Slot = "set" | "empty" | "unsure";

function slotOf(model: ModelMeta, row: Row, name: string): Slot {
  const head = name.split(".")[0]!;
  const v = row[head];
  if (isEmpty(v)) return "empty";
  if (name.includes(".")) return "unsure";
  const f = fieldOf(model, name);
  if (!f) return "unsure";
  if (f.kind === "ref") return f.to ? "set" : "unsure";
  if (f.kind === "text") return String(v) === "" ? "empty" : "set";
  if (f.kind === "longtext")
    return String(v)
      .split("\n")
      .some((l) => l.trim())
      ? "set"
      : "empty";
  return "unsure";
}

/** Whether one chosen field shows stored text alone. */
function fixedField(model: ModelMeta, row: Row, name: string, look: Look, depth: number): boolean {
  const f = fieldOf(model, name);
  if (!f) return false;
  const v = row[name];
  if (f.kind === "text" || f.kind === "longtext")
    // Spaces alone would make the title fall through to the next choice: not worth following.
    return !f.shows_option_of && !f.masked && typeof v === "string" && v.trim() !== "";
  if (f.kind === "ref" && f.to) {
    const target = look.model(f.to);
    const next = look.rows(f.to)?.find((r) => r.id === String(v));
    return !!target && !!next && fixedLabel(target, next, look, depth + 1);
  }
  return false;
}

/** The fields the title of one record (no parent path) is made of, when that is certain. */
function chosenFields(model: ModelMeta, row: Row): string[] | null {
  const hint = model.ui?.title;
  if (hint) {
    // As fillTemplate: the first alternative whose placeholders are all set.
    for (const alt of hint.split("|")) {
      const names = [...alt.matchAll(/\{([\w.]+)\}/g)].map((m) => m[1]!);
      const slots = names.map((n) => slotOf(model, row, n));
      if (slots.includes("empty")) continue;
      if (slots.includes("unsure") || names.length === 0) return null;
      return names;
    }
    return null;
  }
  const names = titleFieldsOf(model);
  if (names.length === 0) return null;
  // "{code} {name}" is written raw.
  if (names.length === 2 && names[0] === "code" && names[1] === "name")
    return isEmpty(row.code) || isEmpty(row.name) ? null : [];
  // A record named by its main ref and date gets "Draft · " in front while it has no number.
  if (stateFieldOf(model) && model.fields.some((f) => f.sequence) && names.length > 0) {
    const f = fieldOf(model, names[0]!);
    if (f && f.kind !== "text" && f.kind !== "longtext") return null;
  }
  return names.every((n) => slotOf(model, row, n) === "set") ? names : null;
}

function fixedOwn(model: ModelMeta, row: Row, look: Look, depth: number): boolean {
  const names = chosenFields(model, row);
  return !!names && names.every((n) => fixedField(model, row, n, look, depth));
}

/**
 * Whether the label of `row` is made of stored text alone, so it reads the same in every language
 * and number format: a text field, `{code} {name}`, a ref to such a record, a parent path. An
 * option label, a select, a date, an amount, a "Draft" prefix or a short id never is.
 */
export function fixedLabel(model: ModelMeta, row: Row, look: Look, depth = 0): boolean {
  if (depth > 3 || !fixedOwn(model, row, look, depth)) return false;
  const parent = fieldOf(model, "parent_id");
  if (!parent || parent.to !== model.name) return true;
  // As titleOf: up to five parents, a parent not loaded ends the path.
  const rows = look.rows(model.name) ?? [];
  const seen = new Set([row.id]);
  let at = row;
  for (let i = 0; i < 5; i++) {
    const pid = at.parent_id;
    if (isEmpty(pid) || seen.has(String(pid))) break;
    const up = rows.find((r) => r.id === String(pid));
    if (!up) break;
    if (!fixedOwn(model, up, look, depth)) return false;
    seen.add(up.id);
    at = up;
  }
  return true;
}

// ---- matching ---------------------------------------------------------------------------------

export type Bucket = { hits: string[]; archived: number; blockers: number };
export type Target = {
  /** Every loaded id, archived or not. */
  ids: ReadonlySet<string>;
  index: ReadonlyMap<string, Bucket>;
  /** Every record the person may read was loaded (no cap, no error). */
  complete: boolean;
};

/**
 * The records of a model by the key of their label: an active record of a fixed label is a hit, an
 * archived one is counted, and an active record whose label is not fixed blocks its name.
 */
export function indexTarget(
  rows: readonly Row[],
  label: (r: Row) => string,
  fixed: (r: Row) => boolean,
  hasActive: boolean,
  complete: boolean,
): Target {
  const index = new Map<string, Bucket>();
  const ids = new Set<string>();
  for (const r of rows) {
    ids.add(r.id);
    const active = !hasActive || r.active !== false;
    const ok = fixed(r);
    if (!active && !ok) continue;
    const key = importKey(label(r));
    let b = index.get(key);
    if (!b) index.set(key, (b = { hits: [], archived: 0, blockers: 0 }));
    if (!ok) b.blockers++;
    else if (active) b.hits.push(r.id);
    else b.archived++;
  }
  return { ids, index, complete };
}

/** The records of `model` the person may read, by the key of their label (`label`: as lists show it). */
export function targetOf(
  model: ModelMeta,
  look: Look,
  label: (r: Row) => string,
  complete: boolean,
): Target {
  // Archived records are told apart as the picker tells them (RefPicker `hasActive`).
  const hasActive = model.fields.some((f) => f.name === "active" && f.kind === "bool");
  return indexTarget(
    look.rows(model.name) ?? [],
    label,
    (r) => fixedLabel(model, r, look),
    hasActive,
    complete,
  );
}

export type Outcome =
  | { kind: "id" }
  | { kind: "one"; id: string }
  | { kind: "none" | "several" | "archived" | "unsure" };

/** What a trimmed cell names: an id, one record, or a problem. Never a guess. */
export function decide(t: Target, v: string): Outcome {
  if (t.ids.has(v)) return { kind: "id" };
  if (!t.complete) return { kind: "unsure" };
  const b = t.index.get(importKey(v));
  if (b && b.blockers > 0) return { kind: "unsure" };
  if (b && b.hits.length === 1) return { kind: "one", id: b.hits[0]! };
  if (b && b.hits.length > 1) return { kind: "several" };
  if (b && b.archived > 0) return { kind: "archived" };
  return { kind: "none" };
}

export type Problem = {
  kind: "none" | "several" | "archived" | "unsure";
  header: string;
  to: string;
  /** The cell's text, trimmed as the server trims it. */
  text: string;
};

/** The file with every name found once replaced by its id, and the first problem of each row. */
export function resolveNames(
  text: string,
  sheet: Sheet,
  columns: readonly RefColumn[],
  targets: ReadonlyMap<string, Target | null>,
): { text: string; problems: Map<number, Problem> } {
  const edits: { start: number; end: number; text: string }[] = [];
  const problems = new Map<number, Problem>();
  for (const rec of sheet.records) {
    for (const col of columns) {
      const cell = rec.cells[col.index];
      if (!cell || !needsLookup(cell.value)) continue;
      const target = targets.get(col.to);
      if (!target) continue; // not readable or not loaded: the server's sentence stays
      const v = rustTrim(cell.value);
      const o = decide(target, v);
      if (o.kind === "id") continue;
      if (o.kind === "one") {
        edits.push({ start: cell.start, end: cell.end, text: o.id });
        continue;
      }
      if (!problems.has(rec.row))
        problems.set(rec.row, { kind: o.kind, header: col.header, to: col.to, text: v });
    }
  }
  return { text: splice(text, edits), problems };
}

/**
 * Whether the server's refusal of a row is about the very cell of the browser's sentence: it ends
 * with `there is no <to> <text>` (crud.rs). A refusal about another cell of the row, even one the
 * browser also left as a name, is something else and is shown after the browser's sentence.
 */
export function sameRefusal(p: Problem, error: string): boolean {
  return error.endsWith(`there is no ${p.to} ${p.text}`);
}

/** A name for a sentence: trimmed, cut at 40 characters with "…". */
export function shortName(s: string): string {
  const v = s.trim();
  const chars = [...v];
  return chars.length > 40 ? `${chars.slice(0, 39).join("")}…` : v;
}

import type { SqlDb, SqlTx } from "./sql.ts";

/**
 * A version for each part of the code that keeps tables in the one database file, and the rules
 * that keep those parts from breaking each other or an older build of the app.
 *
 * The file has two kinds of version. `PRAGMA user_version` is the version of the layout (what kind
 * of file this is) and stays 1: a build that finds a larger one refuses the file, and every build
 * since the first one does so, which is why a module never raises it for an ordinary change. Each
 * module has a version of its own in the table `module_schema`, which a build of this app reads
 * and an older one ignores.
 *
 * A file that an older build made (layout 1, no `module_schema`) is read as it is: the modules
 * that were in that layout say which version their tables have (`legacyVersion`), and nothing is
 * written to the file until a module has something to change. Then the table is made and those
 * versions are recorded in the same transaction. A build that finds nothing to change reads the
 * file and writes nothing, as the build before it did.
 */

/** The layout of the file: the four tables of the first build, and `module_schema` when it was needed. */
export const LAYOUT_VERSION = 1;

export interface SchemaModule {
  /** Lower-case letters and digits, starting with a letter. Names the tables (`<id>_*`) and the meta keys (`<id>.*`). */
  readonly id: string;
  /** The version of its tables that this build works with, 1 or more. */
  readonly version: number;
  /**
   * The version its tables have in a file of layout 1 that has no `module_schema`, for a module
   * whose tables were in the first layout. A module that was not has none, and starts at 0.
   */
  readonly legacyVersion?: number;
  /** Modules that have to be listed before this one: their tables are used by this one's. */
  readonly requires?: readonly string[];
  /** Tables of the first layout that this module owns although their names do not start with `<id>_`. */
  readonly legacyTables?: readonly string[];
  /** Keys of the table `meta` that this module uses although they do not start with `<id>.`. */
  readonly legacyMetaKeys?: readonly string[];
  /**
   * Brings the tables from version `from` (0: there are none) to `version`, in the transaction that
   * `ensureModules` holds. See the rules there.
   */
  migrate(tx: SqlTx, from: number): Promise<void>;
}

/** The refusal of a module list or of a change a module made, with the module named. */
export class SchemaError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SchemaError";
  }
}

const MODULE_ID = /^[a-z][a-z0-9]*$/;

/** Throws for a list of modules that cannot work together; `defineStartup` calls it when the app starts. */
export function checkModules(modules: readonly SchemaModule[]): void {
  const seen = new Set<string>();
  const legacy = new Map<string, string>();
  const legacyKeys = new Map<string, string>();
  for (const m of modules) {
    if (!MODULE_ID.test(m.id)) {
      throw new SchemaError(
        `module id ${JSON.stringify(m.id)} must be lower-case letters and digits`,
      );
    }
    if (seen.has(m.id)) throw new SchemaError(`module ${m.id} is listed twice`);
    if (!Number.isInteger(m.version) || m.version < 1) {
      throw new SchemaError(`module ${m.id}: version must be a whole number from 1`);
    }
    if (m.legacyVersion !== undefined) {
      if (
        !Number.isInteger(m.legacyVersion) ||
        m.legacyVersion < 1 ||
        m.legacyVersion > m.version
      ) {
        throw new SchemaError(`module ${m.id}: legacyVersion must be from 1 to its version`);
      }
    }
    for (const need of m.requires ?? []) {
      if (!seen.has(need)) throw new SchemaError(`module ${m.id} needs ${need}, listed before it`);
    }
    for (const table of m.legacyTables ?? []) {
      const owner = legacy.get(table);
      if (owner !== undefined) {
        throw new SchemaError(`the table ${table} is claimed by ${owner} and by ${m.id}`);
      }
      if (table.startsWith("sqlite_")) throw new SchemaError(`${table} is a table of SQLite`);
      legacy.set(table, m.id);
    }
    for (const key of m.legacyMetaKeys ?? []) {
      const owner = legacyKeys.get(key);
      if (owner !== undefined) {
        throw new SchemaError(`the meta key ${key} is claimed by ${owner} and by ${m.id}`);
      }
      legacyKeys.set(key, m.id);
    }
    seen.add(m.id);
  }
}

/** Whether a table belongs to a module by its name. */
function owns(module: SchemaModule, table: string): boolean {
  return table.startsWith(`${module.id}_`) || (module.legacyTables ?? []).includes(table);
}

const CREATE_MODULE_SCHEMA =
  "CREATE TABLE IF NOT EXISTS module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) WITHOUT ROWID";

type Reader = Pick<SqlTx, "all" | "one">;

interface State {
  layout: number;
  hasTable: boolean;
  /** The versions in `module_schema`; empty where the table is missing. */
  recorded: Map<string, number>;
}

async function readState(q: Reader): Promise<State> {
  const layout = Number(
    (await q.one<{ user_version: number }>("PRAGMA user_version"))?.user_version ?? 0,
  );
  const table = await q.one<{ name: string }>(
    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'module_schema'",
  );
  const recorded = new Map<string, number>();
  if (table !== null) {
    const rows = await q.all<{ module: string; version: number }>(
      "SELECT module, version FROM module_schema",
    );
    for (const row of rows) recorded.set(row.module, Number(row.version));
  }
  return { layout, hasTable: table !== null, recorded };
}

/** The version a module's tables have in this file, as far as the file says. */
function versionIn(state: State, module: SchemaModule): number {
  if (state.hasTable) return state.recorded.get(module.id) ?? 0;
  return state.layout === LAYOUT_VERSION ? (module.legacyVersion ?? 0) : 0;
}

/** Throws for a file of a newer app; answers the modules that are behind. */
function plan(state: State, modules: readonly SchemaModule[]): SchemaModule[] {
  if (state.layout > LAYOUT_VERSION) {
    throw new Error(`The database is from a newer version of the app (schema ${state.layout})`);
  }
  for (const m of modules) {
    const found = versionIn(state, m);
    if (found > m.version) {
      throw new Error(
        `The database is from a newer version of the app (schema ${state.layout}, ${m.id} ${found})`,
      );
    }
  }
  return modules.filter((m) => versionIn(state, m) < m.version);
}

interface Column {
  name: string;
  type: string;
  notnull: number;
  dflt_value: string | null;
  pk: number;
}

interface SchemaObject {
  type: string;
  name: string;
  tbl_name: string;
  sql: string | null;
}

/** What an index is, as far as an older build could trip over it. */
interface IndexInfo {
  unique: boolean;
  partial: boolean;
  /** The column number of each key column, in order; negative for an expression. */
  keys: number[];
}

interface Snapshot {
  /** `PRAGMA user_version`. */
  layout: number;
  objects: Map<string, SchemaObject>;
  columns: Map<string, Column[]>;
  /** The tables that the foreign keys of each table refer to. */
  parents: Map<string, string[]>;
  indexes: Map<string, IndexInfo>;
}

const objectKey = (o: { type: string; name: string }) => `${o.type} ${o.name}`;
const quoted = (name: string) => `"${name.replaceAll('"', '""')}"`;

async function snapshot(tx: SqlTx): Promise<Snapshot> {
  const layout = Number(
    (await tx.one<{ user_version: number }>("PRAGMA user_version"))?.user_version ?? 0,
  );
  const rows = await tx.all<SchemaObject>(
    "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
  );
  const objects = new Map(rows.map((o) => [objectKey(o), o]));
  const columns = new Map<string, Column[]>();
  const parents = new Map<string, string[]>();
  const indexes = new Map<string, IndexInfo>();
  for (const o of rows) {
    if (o.type !== "table") continue;
    columns.set(o.name, await tx.all<Column>(`PRAGMA table_info(${quoted(o.name)})`));
    const keys = await tx.all<{ table: string }>(`PRAGMA foreign_key_list(${quoted(o.name)})`);
    parents.set(
      o.name,
      keys.map((k) => k.table),
    );
    const listed = await tx.all<{ name: string; unique: number; partial: number }>(
      `PRAGMA index_list(${quoted(o.name)})`,
    );
    for (const index of listed) {
      // The indexes that SQLite makes for a key of the table itself are part of the table's text.
      if (!objects.has(objectKey({ type: "index", name: index.name }))) continue;
      const parts = await tx.all<{ cid: number; key: number }>(
        `PRAGMA index_xinfo(${quoted(index.name)})`,
      );
      indexes.set(index.name, {
        unique: Number(index.unique) === 1,
        partial: Number(index.partial) === 1,
        keys: parts.filter((p) => Number(p.key) === 1).map((p) => Number(p.cid)),
      });
    }
  }
  return { layout, objects, columns, parents, indexes };
}

/**
 * What a module may have done to the file, by comparing the file before and after its migration.
 * The point is that a build from before `module_schema` (which cannot know that a table has moved
 * on) still works on the file: it creates its tables with `CREATE TABLE IF NOT EXISTS`, inserts,
 * updates, deletes and reads. So the module:
 *
 * - created objects only on tables it owns (tables named `<id>_*` and its legacy tables), and
 *   changed or removed no object that belongs to another module;
 * - made nothing but tables of its own, indexes, and columns on tables that were there:
 *   - a table of its own may be as it likes, except that it is not a virtual table and has no
 *     foreign key to a table that was in the file before (an older build deletes from that table,
 *     and with foreign keys on the new table would refuse the delete);
 *   - an index is not unique (an older build's insert would fail on a duplicate), has no `WHERE`
 *     and no expression in its key (a function can raise an error on a row that the older build
 *     writes);
 *   - a trigger is refused (it runs on the older build's statements) and so is a view (a view with
 *     the name of a table of the first layout makes the older build's `CREATE TABLE IF NOT EXISTS`
 *     do nothing); so is every other kind of object;
 *   - a table that was there is changed by `ADD COLUMN` only: its text is what it was with column
 *     definitions added, every column that was there is as it was (type, key, NOT NULL), and each
 *     new column is a name, an optional type, and `NOT NULL` and `DEFAULT <plain value>` as wanted,
 *     with no `CHECK`, `REFERENCES`, `COLLATE`, generated value, `UNIQUE`, `PRIMARY KEY`, named
 *     constraint or `DEFAULT (expression)`; a new column that is NOT NULL has a default. Such a
 *     table is never dropped, renamed or rebuilt;
 * - left `PRAGMA user_version` as it was: the layout is `LAYOUT_VERSION`, set by the manager.
 *
 * A change that cannot be made this way needs `LAYOUT_VERSION` raised, which makes every older
 * build refuse the file. What a migration writes into rows is not checked here.
 */
function checkChange(module: SchemaModule, before: Snapshot, after: Snapshot): void {
  const fail: (what: string) => never = (what) => {
    throw new SchemaError(`module ${module.id} ${what}`);
  };
  for (const [key, o] of after.objects) {
    if (!before.objects.has(key) && !owns(module, o.tbl_name)) {
      fail(
        `made the ${o.type} ${o.name} on ${o.tbl_name}, which it does not own (its tables are named ${module.id}_*)`,
      );
    }
  }
  for (const [key, was] of before.objects) {
    const now = after.objects.get(key);
    if (owns(module, was.tbl_name)) {
      if (now === undefined)
        fail(`removed the ${was.type} ${was.name}, which an older build still uses`);
      continue;
    }
    if (now === undefined || now.sql !== was.sql) {
      fail(`changed the ${was.type} ${was.name}, which belongs to another module`);
    }
  }
  const tablesBefore = new Set(
    [...before.objects.values()].filter((o) => o.type === "table").map((o) => o.name.toLowerCase()),
  );
  for (const [key, o] of after.objects) {
    const was = before.objects.get(key);
    // A table that was there is looked at column by column below. Any other object that was there
    // and is as it was is not the module's doing.
    if (was !== undefined && (o.type === "table" || was.sql === o.sql)) continue;
    const verb = was === undefined ? "made" : "changed";
    if (o.type === "table") {
      if (/^\s*CREATE\s+VIRTUAL\b/i.test(o.sql ?? "")) {
        fail(`made the virtual table ${o.name}; an older build may lack what it needs to open it`);
      }
      for (const parent of after.parents.get(o.name) ?? []) {
        if (tablesBefore.has(parent.toLowerCase())) {
          fail(
            `made the table ${o.name} with a foreign key to ${parent}, which was in the file before; an older build that deletes from ${parent} would be refused`,
          );
        }
      }
    } else if (o.type === "index") {
      const index = after.indexes.get(o.name);
      if (index === undefined) {
        fail(`${verb} the index ${o.name}, which cannot be read`);
      } else if (index.unique) {
        fail(
          `${verb} the unique index ${o.name}; an older build inserts rows that it would refuse`,
        );
      } else if (index.partial) {
        fail(
          `${verb} the index ${o.name} with a WHERE clause, which is evaluated on an older build's rows`,
        );
      } else if (index.keys.some((cid) => cid < 0)) {
        fail(
          `${verb} the index ${o.name} on an expression, which is evaluated on an older build's rows`,
        );
      }
    } else if (o.type === "trigger") {
      fail(`${verb} the trigger ${o.name}; it would run on the statements of an older build`);
    } else if (o.type === "view") {
      fail(
        `${verb} the view ${o.name}; a view takes the place of a table of that name for an older build`,
      );
    } else {
      fail(`${verb} the ${o.type} ${o.name}, a kind of object that is not allowed`);
    }
  }
  for (const [table, was] of before.columns) {
    if (!owns(module, table)) continue;
    const now = after.columns.get(table) ?? [];
    for (const column of was) {
      const kept = now.find((c) => c.name === column.name);
      if (
        kept === undefined ||
        kept.type !== column.type ||
        kept.pk !== column.pk ||
        kept.notnull !== column.notnull
      ) {
        fail(
          `changed the column ${table}.${column.name}; a change that an older build cannot read raises the layout version instead`,
        );
      }
    }
    for (const column of now) {
      if (
        !was.some((c) => c.name === column.name) &&
        column.notnull === 1 &&
        column.dflt_value === null
      ) {
        fail(`added the column ${table}.${column.name} as NOT NULL without a default`);
      }
    }
    // The columns say what the table has. Its text says what else it is made of: a key over two
    // columns, a CHECK, a UNIQUE, WITHOUT ROWID, STRICT, and what the new columns carry.
    const textBefore = before.objects.get(`table ${table}`)?.sql;
    const textAfter = after.objects.get(`table ${table}`)?.sql;
    if (
      typeof textBefore === "string" &&
      typeof textAfter === "string" &&
      textBefore !== textAfter
    ) {
      const added = insertion(textBefore, textAfter);
      if (added === null) {
        fail(
          `rebuilt the table ${table}; an older build only works with it if columns are all that was added`,
        );
      }
      const problem = columnsProblem(added);
      if (problem !== null) {
        fail(
          `added the column ${table}.${problem.column} with ${problem.why}; a new column is a plain one that may be empty or has a default`,
        );
      }
    }
  }
  if (after.layout !== before.layout) {
    fail("changed PRAGMA user_version; the layout version is LAYOUT_VERSION, set by the manager");
  }
}

/**
 * The text that `now` has and `was` has not, when `now` is `was` with one piece added; else null.
 * `ALTER TABLE ... ADD COLUMN` puts `, <column definition>` after the last column and before the
 * constraints of the table (a key over two columns, WITHOUT ROWID), so the piece starts with a comma.
 */
function insertion(was: string, now: string): string | null {
  if (now.length <= was.length) return null;
  let common = 0;
  while (common < was.length && was[common] === now[common]) common++;
  for (let at = common; at >= 0; at--) {
    if (!now.endsWith(was.slice(at))) continue;
    const piece = now.slice(at, now.length - (was.length - at));
    if (piece.startsWith(",")) return piece;
  }
  return null;
}

interface Token {
  kind: "word" | "name" | "string" | "blob" | "number" | "mark";
  text: string;
}

const NUMBER = /(?:0[xX][0-9a-fA-F]+|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)/y;
const WORD = /[A-Za-z_\u0080-￿][A-Za-z0-9_$\u0080-￿]*/y;
const SPACE = /\s+/y;

/** Reads the words, names, literals and marks of a piece of a `CREATE TABLE`; null where it cannot. */
function tokenise(sql: string): Token[] | null {
  const out: Token[] = [];
  const at = (re: RegExp, from: number): string | null => {
    re.lastIndex = from;
    return re.exec(sql)?.[0] ?? null;
  };
  let i = 0;
  while (i < sql.length) {
    const space = at(SPACE, i);
    if (space !== null) {
      i += space.length;
      continue;
    }
    if (sql.startsWith("--", i)) {
      const end = sql.indexOf("\n", i);
      i = end < 0 ? sql.length : end + 1;
      continue;
    }
    if (sql.startsWith("/*", i)) {
      const end = sql.indexOf("*/", i + 2);
      if (end < 0) return null;
      i = end + 2;
      continue;
    }
    const c = sql[i] as string;
    if (c === "'" || c === '"' || c === "`" || c === "[") {
      const close = c === "[" ? "]" : c;
      let j = i + 1;
      for (;;) {
        const end = sql.indexOf(close, j);
        if (end < 0) return null;
        if (close !== "]" && sql[end + 1] === close) {
          j = end + 2;
          continue;
        }
        j = end;
        break;
      }
      out.push({ kind: c === "'" ? "string" : "name", text: sql.slice(i + 1, j) });
      i = j + 1;
      continue;
    }
    const word = at(WORD, i);
    if (word !== null) {
      if (/^[xX]$/.test(word) && sql[i + 1] === "'") {
        const end = sql.indexOf("'", i + 2);
        if (end < 0) return null;
        out.push({ kind: "blob", text: sql.slice(i + 2, end) });
        i = end + 1;
        continue;
      }
      out.push({ kind: "word", text: word });
      i += word.length;
      continue;
    }
    const number = at(NUMBER, i);
    if (number !== null) {
      out.push({ kind: "number", text: number });
      i += number.length;
      continue;
    }
    out.push({ kind: "mark", text: c });
    i++;
  }
  return out;
}

const isWord = (t: Token | undefined, word: string): boolean =>
  t !== undefined && t.kind === "word" && t.text.toUpperCase() === word;
const isMark = (t: Token | undefined, mark: string): boolean =>
  t !== undefined && t.kind === "mark" && t.text === mark;

/** The words that start a constraint of a column. A type name does not contain them. */
const CONSTRAINT_WORDS = new Map<string, string>([
  ["CONSTRAINT", "a named constraint"],
  ["PRIMARY", "a primary key"],
  ["UNIQUE", "a UNIQUE constraint"],
  ["CHECK", "a CHECK constraint"],
  ["COLLATE", "a collation"],
  ["REFERENCES", "a foreign key"],
  ["GENERATED", "a generated value"],
  ["AS", "a generated value"],
  ["ON", "an ON CONFLICT clause"],
]);
const PLAIN_DEFAULT_WORDS = new Set([
  "TRUE",
  "FALSE",
  "CURRENT_TIME",
  "CURRENT_DATE",
  "CURRENT_TIMESTAMP",
]);

/** Why the column definitions in `, a TEXT, b INTEGER NOT NULL DEFAULT 0` are not all plain; null if they are. */
function columnsProblem(added: string): { column: string; why: string } | null {
  const tokens = tokenise(added);
  const unreadable = { column: "?", why: "text that cannot be read" };
  if (tokens === null) return unreadable;
  const defs: Token[][] = [];
  let depth = 0;
  for (const t of tokens) {
    if (isMark(t, "(")) depth++;
    if (isMark(t, ")")) depth--;
    if (depth < 0) return unreadable;
    if (depth === 0 && isMark(t, ",")) {
      defs.push([]);
      continue;
    }
    const current = defs[defs.length - 1];
    if (current === undefined) return unreadable;
    current.push(t);
  }
  if (depth !== 0 || defs.length === 0) return unreadable;
  for (const def of defs) {
    const why = plainColumn(def);
    if (why !== null) return { column: def[0]?.text ?? "?", why };
  }
  return null;
}

/** Null when the definition is a name, an optional type, and NOT NULL and DEFAULT of a plain value only. */
function plainColumn(def: Token[]): string | null {
  const first = def[0];
  if (first === undefined || (first.kind !== "word" && first.kind !== "name")) return "no name";
  let i = 1;
  // The type: words that do not start a constraint, then a size.
  for (;;) {
    const t = def[i];
    if (t === undefined || t.kind !== "word") break;
    const upper = t.text.toUpperCase();
    if (CONSTRAINT_WORDS.has(upper) || upper === "NOT" || upper === "NULL" || upper === "DEFAULT") {
      break;
    }
    i++;
  }
  const sign = (): void => {
    if (isMark(def[i], "+") || isMark(def[i], "-")) i++;
  };
  if (isMark(def[i], "(")) {
    const badSize = "a type size that is not a number";
    i++;
    sign();
    if (def[i]?.kind !== "number") return badSize;
    i++;
    if (isMark(def[i], ",")) {
      i++;
      sign();
      if (def[i]?.kind !== "number") return badSize;
      i++;
    }
    if (!isMark(def[i], ")")) return badSize;
    i++;
  }
  let notNull = false;
  let value = false;
  while (i < def.length) {
    const t = def[i] as Token;
    if (isWord(t, "NOT") && isWord(def[i + 1], "NULL")) {
      notNull = true;
      i += 2;
    } else if (isWord(t, "NULL")) {
      i++;
    } else if (isWord(t, "DEFAULT")) {
      i++;
      const v = def[i];
      if (isMark(v, "+") || isMark(v, "-")) {
        if (def[i + 1]?.kind !== "number") return "a default that is not a plain value";
        i += 2;
        value = true;
      } else if (v?.kind === "number" || v?.kind === "string" || v?.kind === "blob") {
        i++;
        value = true;
      } else if (isWord(v, "NULL")) {
        i++;
      } else if (v?.kind === "word" && PLAIN_DEFAULT_WORDS.has(v.text.toUpperCase())) {
        i++;
        value = true;
      } else {
        return "a default that is not a plain value";
      }
    } else {
      const named = t.kind === "word" ? CONSTRAINT_WORDS.get(t.text.toUpperCase()) : undefined;
      return named ?? `the text ${t.text}`;
    }
  }
  return notNull && !value ? "NOT NULL and no default" : null;
}

export interface EnsureOptions {
  /**
   * Called after the file has been looked at without a lock and before the lock is taken. For a
   * test that has to let a second connection in between; the code does not use it.
   */
  betweenLookAndLock?: () => Promise<void>;
}

/**
 * Brings the file to what `modules` need, and refuses a file that is newer than they know.
 *
 * Nothing is written when no module is behind and the layout is current: the file is only read.
 * Otherwise one transaction takes the write lock first (`BEGIN IMMEDIATE`) and then reads the file
 * again, because a second process (the screens and the location task can both open the file) may
 * have done the work while this one was waiting, and a migration that is not safe to run twice
 * must not run twice. In that transaction: `module_schema` is made if it is missing, the versions
 * of a layout-1 file are recorded for the modules that were in it, every module that is behind is
 * migrated in list order and checked (`checkChange`), its version is recorded, and the layout
 * version is set. Any failure rolls all of it back. A file of a newer version is refused for every
 * module at once, on purpose: the modules share one file and one transaction.
 */
export async function ensureModules(
  db: SqlDb,
  modules: readonly SchemaModule[],
  options: EnsureOptions = {},
): Promise<void> {
  checkModules(modules);
  const first = await readState(db);
  const behind = plan(first, modules);
  if (behind.length === 0 && first.layout === LAYOUT_VERSION) return;
  await options.betweenLookAndLock?.();

  await db.transaction(async (tx) => {
    const state = await readState(tx);
    const todo = plan(state, modules);
    if (todo.length === 0 && state.layout === LAYOUT_VERSION) return;

    if (!state.hasTable) {
      await tx.exec(CREATE_MODULE_SCHEMA);
      if (state.layout === LAYOUT_VERSION) {
        for (const m of modules) {
          if (m.legacyVersion === undefined) continue;
          await tx.run(
            "INSERT INTO module_schema (module, version) VALUES (?, ?) ON CONFLICT(module) DO NOTHING",
            [m.id, m.legacyVersion],
          );
          state.recorded.set(m.id, m.legacyVersion);
        }
      }
    }

    for (const m of todo) {
      const from = state.recorded.get(m.id) ?? 0;
      const before = await snapshot(tx);
      await m.migrate(tx, from);
      checkChange(m, before, await snapshot(tx));
      await tx.run(
        "INSERT INTO module_schema (module, version) VALUES (?, ?) ON CONFLICT(module) DO UPDATE SET version = excluded.version",
        [m.id, m.version],
      );
      state.recorded.set(m.id, m.version);
    }
    if (state.layout < LAYOUT_VERSION) await tx.exec(`PRAGMA user_version = ${LAYOUT_VERSION}`);
  });
}

/** The two methods of the table `meta` that the host and the modules reach it through. */
export interface MetaAccess {
  getMeta(key: string): Promise<string | null>;
  setMeta(key: string, value: string | null): Promise<void>;
}

/** Whether a key of the table `meta` is the module's: `<id>.<name>`, or one of its legacy keys. */
export function moduleMetaKeyAllowed(
  module: Pick<SchemaModule, "id" | "legacyMetaKeys">,
  key: string,
): boolean {
  return key.startsWith(`${module.id}.`) || (module.legacyMetaKeys ?? []).includes(key);
}

/** The table `meta` as one module may use it: any other key throws. */
export function scopedMeta(
  meta: MetaAccess,
  module: Pick<SchemaModule, "id" | "legacyMetaKeys">,
): MetaAccess {
  const check = (key: string): void => {
    if (!moduleMetaKeyAllowed(module, key)) {
      throw new SchemaError(
        `module ${module.id} may use the meta keys ${module.id}.* only, not ${JSON.stringify(key)}`,
      );
    }
  };
  return {
    getMeta: (key) => {
      check(key);
      return meta.getMeta(key);
    },
    setMeta: (key, value) => {
      check(key);
      return meta.setMeta(key, value);
    },
  };
}

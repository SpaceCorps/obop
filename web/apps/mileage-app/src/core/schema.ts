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

interface Snapshot {
  objects: Map<string, SchemaObject>;
  columns: Map<string, Column[]>;
}

const objectKey = (o: { type: string; name: string }) => `${o.type} ${o.name}`;

async function snapshot(tx: SqlTx): Promise<Snapshot> {
  const rows = await tx.all<SchemaObject>(
    "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
  );
  const objects = new Map(rows.map((o) => [objectKey(o), o]));
  const columns = new Map<string, Column[]>();
  for (const o of rows) {
    if (o.type !== "table") continue;
    const quoted = `"${o.name.replaceAll('"', '""')}"`;
    columns.set(o.name, await tx.all<Column>(`PRAGMA table_info(${quoted})`));
  }
  return { objects, columns };
}

/**
 * What a module may have done to the file, by comparing the file before and after its migration:
 *
 * - it created objects only on tables it owns (tables named `<id>_*` and its legacy tables), and
 *   changed or removed no object that belongs to another module;
 * - it left the tables it owns so that code of an older version still works: every column that
 *   was there is still there with the same type and key, and a column that is new is nullable or
 *   has a default. An older build of the app does not read `module_schema`, so it cannot know that
 *   a table has moved on; this is what keeps it working. A change that cannot be made this way
 *   needs `LAYOUT_VERSION` raised, which makes every older build refuse the file.
 */
function checkChange(module: SchemaModule, before: Snapshot, after: Snapshot): void {
  const fail = (what: string): never => {
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
  }
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

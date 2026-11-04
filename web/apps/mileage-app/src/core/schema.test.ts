import { afterEach, describe, expect, test } from "vite-plus/test";
import {
  LAYOUT_VERSION,
  SchemaError,
  ensureModules,
  moduleMetaKeyAllowed,
  scopedMeta,
  type SchemaModule,
} from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { memoryDb } from "../test-support/memory-db.ts";
import { tempDatabase } from "../test-support/temp-db.ts";

// The manager of the versions of the parts of the app that keep tables in the file, tested with
// small made-up modules. The modules of the app are in `schema-upgrade.test.ts`.

/** A module that makes the tables it is given and counts how often it ran. */
function module(
  id: string,
  options: Partial<SchemaModule> & { steps?: Record<number, string[]> } = {},
) {
  const { steps = { 1: [`CREATE TABLE ${id}_items (id TEXT PRIMARY KEY NOT NULL)`] }, ...rest } =
    options;
  const ran: number[] = [];
  const m: SchemaModule & { ran: number[] } = {
    id,
    version: Math.max(...Object.keys(steps).map(Number)),
    ...rest,
    ran,
    async migrate(tx, from) {
      ran.push(from);
      for (const [version, statements] of Object.entries(steps)) {
        if (Number(version) <= from) continue;
        for (const statement of statements) await tx.exec(statement);
      }
    },
  };
  return m;
}

const tables = async (db: SqlDb) =>
  (
    await db.all<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
    )
  ).map((r) => r.name);
const versions = async (db: SqlDb) =>
  Object.fromEntries(
    (
      await db.all<{ module: string; version: number }>("SELECT module, version FROM module_schema")
    ).map((r) => [r.module, r.version]),
  );
const layout = async (db: SqlDb) =>
  Number((await db.one<{ user_version: number }>("PRAGMA user_version"))?.user_version);

/** The statements that change something, and the transactions, that a database was given. */
function watched(db: SqlDb) {
  const log: string[] = [];
  const spy: SqlDb = {
    ...db,
    exec: (sql) => {
      log.push(`exec ${sql.slice(0, 40)}`);
      return db.exec(sql);
    },
    run: (sql, params) => {
      log.push(`run ${sql.slice(0, 40)}`);
      return db.run(sql, params);
    },
    transaction: (fn) => {
      log.push("transaction");
      return db.transaction(fn);
    },
  };
  return { spy, log };
}

describe("a new file", () => {
  test("gets every module in list order, its version recorded, and layout 1", async () => {
    const db = memoryDb();
    const a = module("a", {
      steps: {
        1: ["CREATE TABLE a_items (id TEXT)"],
        2: ["ALTER TABLE a_items ADD COLUMN note TEXT"],
      },
    });
    const b = module("b", { requires: ["a"] });
    await ensureModules(db, [a, b]);
    expect(a.ran).toEqual([0]);
    expect(b.ran).toEqual([0]);
    expect(await tables(db)).toEqual(["a_items", "b_items", "module_schema"]);
    expect(await versions(db)).toEqual({ a: 2, b: 1 });
    expect(await layout(db)).toBe(LAYOUT_VERSION);
    expect(LAYOUT_VERSION).toBe(1);
  });

  test("is not touched again by a second call: it is read, not written", async () => {
    const db = memoryDb();
    const m = module("a");
    await ensureModules(db, [m]);
    const { spy, log } = watched(db);
    await ensureModules(spy, [m]);
    expect(m.ran).toEqual([0]);
    expect(log).toEqual([]);
  });

  test("is made all or nothing: a module that fails takes the earlier ones back", async () => {
    const db = memoryDb();
    const a = module("a");
    const b: SchemaModule = {
      id: "b",
      version: 1,
      migrate: async (tx) => {
        await tx.exec("CREATE TABLE b_items (id TEXT)");
        throw new Error("the disk is full");
      },
    };
    await expect(ensureModules(db, [a, b])).rejects.toThrow("the disk is full");
    expect(await tables(db)).toEqual([]);
    expect(await layout(db)).toBe(0);
    // And the next start tries again from the beginning.
    const again = module("b");
    await ensureModules(db, [a, again]);
    expect(await versions(db)).toEqual({ a: 1, b: 1 });
  });
});

describe("a module that moves to a later version", () => {
  test("runs from the version it is at, and only it", async () => {
    const db = memoryDb();
    const a1 = module("a", { steps: { 1: ["CREATE TABLE a_items (id TEXT)"] } });
    const b = module("b");
    await ensureModules(db, [a1, b]);
    const a2 = module("a", {
      steps: {
        1: ["CREATE TABLE a_items (id TEXT)"],
        2: ["ALTER TABLE a_items ADD COLUMN note TEXT"],
      },
    });
    const b2 = module("b");
    await ensureModules(db, [a2, b2]);
    expect(a2.ran).toEqual([1]);
    expect(b2.ran).toEqual([]);
    expect(await versions(db)).toEqual({ a: 2, b: 1 });
    expect(await layout(db)).toBe(1);
  });

  test("a module that is new to a file that has others starts at 0", async () => {
    const db = memoryDb();
    await ensureModules(db, [module("a")]);
    const c = module("c");
    await ensureModules(db, [module("a"), c]);
    expect(c.ran).toEqual([0]);
    expect(await versions(db)).toEqual({ a: 1, c: 1 });
  });
});

describe("a file that is newer than the app", () => {
  test("is refused when its layout is newer, before anything is written", async () => {
    const db = memoryDb();
    await db.exec("PRAGMA user_version = 99");
    const m = module("a");
    await expect(ensureModules(db, [m])).rejects.toThrow(/newer version of the app \(schema 99\)/);
    expect(m.ran).toEqual([]);
    expect(await tables(db)).toEqual([]);
  });

  test("is refused when a module's version in it is newer, for every module", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      module("a", {
        steps: {
          1: ["CREATE TABLE a_items (id TEXT)"],
          2: ["ALTER TABLE a_items ADD COLUMN n TEXT"],
        },
      }),
      module("b"),
    ]);
    const a1 = module("a", { steps: { 1: ["CREATE TABLE a_items (id TEXT)"] } });
    const b = module("b");
    await expect(ensureModules(db, [a1, b])).rejects.toThrow(/newer version of the app.*a 2/);
    expect(b.ran).toEqual([]);
    expect(await versions(db)).toEqual({ a: 2, b: 1 });
  });
});

describe("a file of layout 1 that no module-aware build has touched", () => {
  /** The tables of the first layout, as the first build made them, and layout 1. */
  async function installed() {
    const db = memoryDb();
    await db.exec(`
      CREATE TABLE meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) WITHOUT ROWID;
      CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL);
      INSERT INTO meta (key, value) VALUES ('owner', 'x');
      INSERT INTO trip (id) VALUES ('t1');
      PRAGMA user_version = 1;
    `);
    return db;
  }
  const host = () =>
    module("host", {
      legacyVersion: 1,
      legacyTables: ["meta"],
      steps: { 1: ["CREATE TABLE IF NOT EXISTS meta (key TEXT)"] },
    });
  const mileage = () =>
    module("mileage", {
      legacyVersion: 1,
      requires: ["host"],
      legacyTables: ["trip"],
      steps: { 1: ["CREATE TABLE IF NOT EXISTS trip (id TEXT)"] },
    });

  test("is read and not written when no module has anything to do", async () => {
    const db = await installed();
    const { spy, log } = watched(db);
    const h = host();
    const m = mileage();
    await ensureModules(spy, [h, m]);
    expect(log).toEqual([]);
    expect(h.ran).toEqual([]);
    expect(m.ran).toEqual([]);
    expect(await tables(db)).toEqual(["meta", "trip"]);
  });

  test("is adopted when a module is new: the versions of the old tables are recorded, the new module starts at 0", async () => {
    const db = await installed();
    const h = host();
    const m = mileage();
    const extra = module("extra", { requires: ["mileage"] });
    await ensureModules(db, [h, m, extra]);
    expect(h.ran).toEqual([]);
    expect(m.ran).toEqual([]);
    expect(extra.ran).toEqual([0]);
    expect(await versions(db)).toEqual({ host: 1, mileage: 1, extra: 1 });
    expect(await tables(db)).toEqual(["extra_items", "meta", "module_schema", "trip"]);
    // The rows of the old tables are as they were.
    expect(await db.all("SELECT key, value FROM meta")).toEqual([{ key: "owner", value: "x" }]);
    expect(await db.all("SELECT id FROM trip")).toEqual([{ id: "t1" }]);
    expect(await layout(db)).toBe(1);
  });

  test("a module without a legacy version is not adopted, whatever tables it has", async () => {
    const db = await installed();
    const stranger = module("stranger");
    await ensureModules(db, [host(), mileage(), stranger]);
    expect(stranger.ran).toEqual([0]);
  });

  test("a module that moves on is migrated from the version of the old tables", async () => {
    const db = await installed();
    const m2 = module("mileage", {
      legacyVersion: 1,
      requires: ["host"],
      legacyTables: ["trip"],
      steps: {
        1: ["CREATE TABLE IF NOT EXISTS trip (id TEXT)"],
        2: ["ALTER TABLE trip ADD COLUMN note TEXT"],
      },
    });
    await ensureModules(db, [host(), m2]);
    expect(m2.ran).toEqual([1]);
    expect(await versions(db)).toEqual({ host: 1, mileage: 2 });
  });
});

describe("what a module may do to the file", () => {
  const refused = async (steps: Record<number, string[]>, ...more: SchemaModule[]) => {
    const db = memoryDb();
    for (const m of more) await ensureModules(db, [m]);
    const rogue = module("rogue", { steps });
    return { db, error: await ensureModules(db, [...more, rogue]).catch((e: unknown) => e) };
  };

  test("makes tables named after itself, and indexes and views on them", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      module("a", {
        steps: {
          1: [
            "CREATE TABLE a_items (id TEXT PRIMARY KEY, n INTEGER)",
            "CREATE INDEX a_items_n ON a_items (n)",
            "CREATE VIEW a_view AS SELECT id FROM a_items",
            "CREATE TABLE a_log (id INTEGER PRIMARY KEY AUTOINCREMENT, text TEXT)",
          ],
        },
      }),
    ]);
    expect(await tables(db)).toEqual(["a_items", "a_log", "module_schema"]);
  });

  test("does not make a table that is not named after itself", async () => {
    const { db, error } = await refused({ 1: ["CREATE TABLE trip (id TEXT)"] });
    expect(error).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(/module rogue made the table trip.*does not own/);
    expect(await tables(db)).toEqual([]);
  });

  test("owns the legacy tables it declares, and no others", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      module("a", { legacyTables: ["trip"], steps: { 1: ["CREATE TABLE trip (id TEXT)"] } }),
    ]);
    expect(await tables(db)).toEqual(["module_schema", "trip"]);
    const error = await ensureModules(db, [
      module("b", { steps: { 1: ["CREATE TABLE point (id TEXT)"] } }),
    ]).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(SchemaError);
  });

  test("does not put an index or a trigger on a table of another module", async () => {
    const a = module("a");
    const { error } = await refused({ 1: ["CREATE INDEX rogue_idx ON a_items (id)"] }, a);
    expect((error as Error).message).toMatch(/does not own/);
    const { error: trigger } = await refused(
      { 1: ["CREATE TRIGGER rogue_t AFTER INSERT ON a_items BEGIN SELECT 1; END"] },
      module("a"),
    );
    expect((trigger as Error).message).toMatch(/does not own/);
  });

  test("does not change a table of another module", async () => {
    const { error } = await refused(
      { 1: ["ALTER TABLE a_items ADD COLUMN extra TEXT"] },
      module("a"),
    );
    expect(error).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(/changed the table a_items.*another module/);
  });

  test("does not remove a table of another module", async () => {
    const { error } = await refused({ 1: ["DROP TABLE a_items"] }, module("a"));
    expect((error as Error).message).toMatch(/another module/);
  });

  describe("moving a table of its own to a later version, so that an older version of the code still works", () => {
    const first = {
      1: [
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0)",
      ],
    };
    const later = async (statement: string) => {
      const db = memoryDb();
      await ensureModules(db, [module("a", { steps: first })]);
      const a2 = module("a", { steps: { ...first, 2: [statement] } });
      const error = await ensureModules(db, [a2]).catch((e: unknown) => e);
      return { db, error };
    };

    test.each([
      ["a column that may be empty", "ALTER TABLE a_items ADD COLUMN more TEXT"],
      ["a column with a default", "ALTER TABLE a_items ADD COLUMN flag INTEGER NOT NULL DEFAULT 0"],
      ["a table of its own", "CREATE TABLE a_more (id TEXT)"],
      ["an index", "CREATE INDEX a_items_note ON a_items (note)"],
    ])("may add %s", async (_, statement) => {
      const { db, error } = await later(statement);
      expect(error).toBeUndefined();
      expect((await versions(db)).a).toBe(2);
    });

    test.each([
      ["drop a column", "ALTER TABLE a_items DROP COLUMN note"],
      ["rename a column", "ALTER TABLE a_items RENAME COLUMN note TO remark"],
      ["rename the table", "ALTER TABLE a_items RENAME TO a_things"],
      ["drop the table", "DROP TABLE a_items"],
      [
        "retype a column by making the table again",
        "CREATE TABLE a_new (id TEXT PRIMARY KEY NOT NULL, note INTEGER, n INTEGER NOT NULL DEFAULT 0); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items",
      ],
      [
        "make a column NOT NULL by making the table again",
        "CREATE TABLE a_new (id TEXT PRIMARY KEY NOT NULL, note TEXT NOT NULL DEFAULT '', n INTEGER NOT NULL DEFAULT 0); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items",
      ],
    ])("may not %s", async (_, statement) => {
      const { db, error } = await later(statement);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      // Taken back: the file is at version 1 and the table is as it was.
      expect((await versions(db)).a).toBe(1);
      expect(
        (await db.all<{ name: string }>("PRAGMA table_info(a_items)")).map((c) => c.name),
      ).toEqual(["id", "note", "n"]);
    });

    test("may not add a column that is NOT NULL without a default (SQLite refuses it too)", async () => {
      const { error } = await later("ALTER TABLE a_items ADD COLUMN must TEXT NOT NULL");
      expect(error).toBeInstanceOf(Error);
    });
  });
});

describe("a list of modules that cannot work", () => {
  test.each<[string, SchemaModule[], RegExp]>([
    ["an id with a capital", [module("Mile")], /lower-case/],
    ["an id with an underscore", [module("a_b")], /lower-case/],
    ["the same id twice", [module("a"), module("a")], /twice/],
    [
      "a module that needs one listed after it",
      [module("b", { requires: ["a"] }), module("a")],
      /needs a/,
    ],
    ["a legacy version beyond its version", [module("a", { legacyVersion: 2 })], /legacyVersion/],
    [
      "a legacy table claimed twice",
      [module("a", { legacyTables: ["t"] }), module("b", { legacyTables: ["t"] })],
      /claimed by a and by b/,
    ],
    [
      "a table of SQLite as a legacy table",
      [module("a", { legacyTables: ["sqlite_sequence"] })],
      /SQLite/,
    ],
  ])("%s is refused before anything is written", async (_, modules, message) => {
    const db = memoryDb();
    const { spy, log } = watched(db);
    await expect(ensureModules(spy, modules)).rejects.toThrow(message);
    expect(log).toEqual([]);
  });
});

describe("the keys of the table meta", () => {
  const a = { id: "a", legacyMetaKeys: ["active_trip"] };

  test("a module's keys start with its id and a dot; its legacy keys are the exception", () => {
    expect(moduleMetaKeyAllowed(a, "a.theme")).toBe(true);
    expect(moduleMetaKeyAllowed(a, "active_trip")).toBe(true);
    expect(moduleMetaKeyAllowed(a, "owner")).toBe(false);
    expect(moduleMetaKeyAllowed(a, "ab.theme")).toBe(false);
    expect(moduleMetaKeyAllowed(a, "a")).toBe(false);
  });

  test("a scoped table refuses any other key, for reading and for writing", async () => {
    const seen: string[] = [];
    const meta = {
      getMeta: async (key: string) => {
        seen.push(`get ${key}`);
        return null;
      },
      setMeta: async (key: string) => {
        seen.push(`set ${key}`);
      },
    };
    const scoped = scopedMeta(meta, a);
    await scoped.getMeta("a.theme");
    await scoped.setMeta("active_trip", "t1");
    expect(() => scoped.getMeta("owner")).toThrow(SchemaError);
    expect(() => scoped.setMeta("b.theme", null)).toThrow(/a\.\* only/);
    expect(seen).toEqual(["get a.theme", "set active_trip"]);
  });
});

describe("two connections to one file", () => {
  const files: ReturnType<typeof tempDatabase>[] = [];
  afterEach(() => {
    for (const f of files.splice(0)) f.close();
  });

  test("the second one finds what the first did while it was between its look and its lock, and does not do it again", async () => {
    const file = tempDatabase();
    files.push(file);
    const screens = file.connect().db;
    const task = file.connect().db;
    const m = module("a", {
      steps: {
        1: ["CREATE TABLE a_items (id TEXT)"],
        2: ["ALTER TABLE a_items ADD COLUMN n TEXT"],
      },
    });

    await ensureModules(task, [m], {
      // The screens' connection gets in after the task has seen a new file and before it has the lock.
      betweenLookAndLock: () => ensureModules(screens, [m]),
    });

    // Run once, by the connection that was first to the lock, and not again by the other.
    expect(m.ran).toEqual([0]);
    expect(await versions(task)).toEqual({ a: 2 });
    expect(await tables(screens)).toEqual(["a_items", "module_schema"]);
  });

  test("a file made by one connection is read by the other without a write", async () => {
    const file = tempDatabase();
    files.push(file);
    const first = file.connect().db;
    const second = file.connect().db;
    const m = module("a");
    await ensureModules(first, [m]);
    const { spy, log } = watched(second);
    await ensureModules(spy, [m]);
    expect(log).toEqual([]);
    expect(m.ran).toEqual([0]);
  });
});

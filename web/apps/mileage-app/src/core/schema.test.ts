import { afterEach, describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "./host-state.ts";
import {
  LAYOUT_VERSION,
  SchemaError,
  appended,
  columnsProblem,
  ensureModules,
  moduleMetaKeyAllowed,
  scopedMeta,
  storedDefinition,
  type SchemaModule,
} from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { MILEAGE_SCHEMA } from "./sql-store.ts";
import { memoryDb } from "../test-support/memory-db.ts";
import { tempDatabase } from "../test-support/temp-db.ts";
import { watched } from "../test-support/watched-db.ts";

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

describe("the versions of the first layout are recorded only for a file of layout 1 that has no version table", () => {
  /** A module that was in the first layout, at version 1 there, and has moved on since. */
  const legacy = (steps: Record<number, string[]>, legacyVersion = 1) =>
    module("a", { legacyVersion, steps });
  const columns = async (db: SqlDb) =>
    (await db.all<{ name: string }>("PRAGMA table_info(a_items)")).map((c) => c.name);

  test("a new file (layout 0) is not read as one of the first layout: the module starts from nothing", async () => {
    const db = memoryDb();
    const a = legacy({ 1: ["CREATE TABLE a_items (id TEXT)"] });
    await ensureModules(db, [a]);
    expect(a.ran).toEqual([0]);
    expect(await tables(db)).toEqual(["a_items", "module_schema"]);
    expect(await versions(db)).toEqual({ a: 1 });
  });

  test("a file that has the version table is not read as one of the first layout, whatever it says about a module", async () => {
    const db = memoryDb();
    await ensureModules(db, [module("a", { steps: { 1: ["CREATE TABLE a_items (id TEXT)"] } })]);
    // The module is now one that was in the first layout at version 2, and has a version 3. The
    // file says 1. Its steps 2 and 3 have to run; its legacy version is for files without the table.
    const a3 = legacy(
      {
        1: ["CREATE TABLE a_items (id TEXT)"],
        2: ["ALTER TABLE a_items ADD COLUMN two TEXT"],
        3: ["ALTER TABLE a_items ADD COLUMN three TEXT"],
      },
      2,
    );
    await ensureModules(db, [a3]);
    expect(a3.ran).toEqual([1]);
    expect(await columns(db)).toEqual(["id", "two", "three"]);
    expect(await versions(db)).toEqual({ a: 3 });
  });

  test("a module that is new to a file that has the version table starts from nothing, although it names a legacy version", async () => {
    const db = memoryDb();
    await ensureModules(db, [module("b")]);
    const a = legacy({ 1: ["CREATE TABLE a_items (id TEXT)"] });
    await ensureModules(db, [module("b"), a]);
    expect(a.ran).toEqual([0]);
    expect(await tables(db)).toEqual(["a_items", "b_items", "module_schema"]);
    expect(await versions(db)).toEqual({ a: 1, b: 1 });
  });

  test("the same with the modules of the app: a file made by the host alone gets the outbox's tables when it is added", async () => {
    const db = memoryDb();
    await ensureModules(db, [HOST_SCHEMA]);
    await ensureModules(db, [HOST_SCHEMA, MILEAGE_SCHEMA]);
    expect(await tables(db)).toEqual(["batch", "meta", "module_schema", "point", "trip"]);
    expect(await versions(db)).toEqual({ host: 1, mileage: 1 });
  });
});

describe("what a module may do to the file", () => {
  const refused = async (steps: Record<number, string[]>, ...more: SchemaModule[]) => {
    const db = memoryDb();
    for (const m of more) await ensureModules(db, [m]);
    const rogue = module("rogue", { steps });
    return { db, error: await ensureModules(db, [...more, rogue]).catch((e: unknown) => e) };
  };

  // The view that this test used to make is refused now (see "what a module may add" below): a view
  // is not among the things an older build can live with.
  test("makes tables named after itself, and an index on them", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      module("a", {
        steps: {
          1: [
            "CREATE TABLE a_items (id TEXT PRIMARY KEY, n INTEGER)",
            "CREATE INDEX a_items_n ON a_items (n)",
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
      module("a", {
        legacyTables: ["trip"],
        legacyDefinitions: { trip: "CREATE TABLE trip (id TEXT)" },
        steps: { 1: ["CREATE TABLE trip (id TEXT)"] },
      }),
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

    // The key of a table is part of what an older build relies on (it inserts with `ON CONFLICT`
    // on the key and reads by it). Both rebuilds keep every column's name, type and NOT NULL, so
    // that it is the comparison of the key, and nothing else, that has to notice. The message is
    // asserted because the check of the table's text refuses these too, with another message.
    test.each([
      [
        "make the key two columns",
        "CREATE TABLE a_new (id TEXT NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (note, id)); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items",
      ],
      [
        "drop the key",
        "CREATE TABLE a_new (id TEXT NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items",
      ],
      [
        "move the key to another column",
        "CREATE TABLE a_new (id TEXT NOT NULL, note TEXT NOT NULL, n INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (note)); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items",
      ],
    ])("may not %s", async (_, statement) => {
      const { db, error } = await later(statement);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(/module a changed the column a_items\.(id|note);/);
      expect((await versions(db)).a).toBe(1);
      const keys = await db.all<{ name: string; pk: number }>("PRAGMA table_info(a_items)");
      expect(keys.map((c) => [c.name, c.pk])).toEqual([
        ["id", 1],
        ["note", 0],
        ["n", 0],
      ]);
    });

    // Each of these keeps every column but one, and changes that one in one respect only, so that
    // it is the comparison of that respect, and nothing else, that has to notice. The message is
    // asserted because the check of the table's text refuses a rebuild as well, with another
    // message, and would hide a comparison that is missing.
    const remake = (columns: string) =>
      `CREATE TABLE a_new (${columns}); DROP TABLE a_items; ALTER TABLE a_new RENAME TO a_items`;
    test.each([
      [
        "the type",
        remake("id TEXT PRIMARY KEY NOT NULL, note INTEGER, n INTEGER NOT NULL DEFAULT 0"),
        "note",
      ],
      [
        "NOT NULL",
        remake("id TEXT PRIMARY KEY NOT NULL, note TEXT NOT NULL, n INTEGER NOT NULL DEFAULT 0"),
        "note",
      ],
      [
        "the default",
        remake("id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 1"),
        "n",
      ],
      [
        "the default, by taking it away",
        remake("id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL"),
        "n",
      ],
      [
        "whether it is a generated column",
        remake(
          "id TEXT PRIMARY KEY NOT NULL, note TEXT GENERATED ALWAYS AS ('x') VIRTUAL, n INTEGER NOT NULL DEFAULT 0",
        ),
        "note",
      ],
      ["the name, in capitals only", "ALTER TABLE a_items RENAME COLUMN note TO NOTE", "note"],
      [
        "its place among the columns",
        remake("id TEXT PRIMARY KEY NOT NULL, n INTEGER NOT NULL DEFAULT 0, note TEXT"),
        "note",
      ],
      // The last column has nothing after it to take its place, so only a column that is missing
      // can show that it is gone.
      ["its presence, when it is the last", "ALTER TABLE a_items DROP COLUMN n", "n"],
    ])("may not change %s of a column that was there", async (_, statement, column) => {
      const { db, error } = await later(statement);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(
        new RegExp(`^module a changed the column a_items\\.${column}; `),
      );
      expect((await versions(db)).a).toBe(1);
      expect(
        (await db.all<{ name: string }>("PRAGMA table_info(a_items)")).map((c) => c.name),
      ).toEqual(["id", "note", "n"]);
    });
  });
});

describe("which tables a module owns", () => {
  // A module owns the tables named `<id>_*` and the ones it lists as legacy tables, by the whole
  // name. A table that only starts with its id (`ab_items` for `a`, `hostage` for `host`) is
  // somebody else's, or nobody's.
  const makes = (m: SchemaModule, table: string): SchemaModule => ({
    ...m,
    async migrate(tx, from) {
      await m.migrate(tx, from);
      await tx.exec(`CREATE TABLE ${table} (id TEXT)`);
    },
  });

  /** The module, with the host listed before it where it needs it. */
  const listed = (m: SchemaModule): SchemaModule[] =>
    m.requires?.includes("host") ? [HOST_SCHEMA, m] : [m];

  test.each([
    ["a", "ab_items", () => module("a", { steps: { 1: [] } })],
    ["host", "hostage", () => HOST_SCHEMA],
    ["mileage", "mileagefoo", () => MILEAGE_SCHEMA],
    ["mileag", "mileage_stop", () => module("mileag", { steps: { 1: [] } })],
    [
      "m, with the legacy table trip,",
      "trip_log",
      () =>
        module("m", {
          legacyTables: ["trip"],
          legacyDefinitions: { trip: "CREATE TABLE trip (id TEXT)" },
          steps: { 1: ["CREATE TABLE trip (id TEXT)"] },
        }),
    ],
    [
      "m, with the legacy table trip,",
      "trips",
      () =>
        module("m", {
          legacyTables: ["trip"],
          legacyDefinitions: { trip: "CREATE TABLE trip (id TEXT)" },
          steps: { 1: ["CREATE TABLE trip (id TEXT)"] },
        }),
    ],
  ])("%s does not make the table %s", async (_, table, build) => {
    const db = memoryDb();
    const error = await ensureModules(db, listed(makes(build(), table))).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(new RegExp(`made the table ${table} .*does not own`));
    expect(await tables(db)).toEqual([]);
  });

  test.each([
    ["a", "a_items", () => module("a", { steps: { 1: [] } })],
    ["host", "host_note", () => HOST_SCHEMA],
    ["mileage", "mileage_stop", () => MILEAGE_SCHEMA],
  ])("%s makes the table %s", async (_, table, build) => {
    const db = memoryDb();
    await ensureModules(db, listed(makes(build(), table)));
    expect(await tables(db)).toContain(table);
  });

  test("a module does not change a table whose name only starts with its id", async () => {
    // `ab` has the table `ab_items`. It starts with `a` but not with `a_`, so it is not module a's.
    const db = memoryDb();
    await ensureModules(db, [module("ab")]);
    const rogue = module("a", { steps: { 1: ["ALTER TABLE ab_items ADD COLUMN extra TEXT"] } });
    const error = await ensureModules(db, [module("ab"), rogue]).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(/changed the table ab_items.*another module/);
  });
});

describe("what a module may add to a file that an older build uses", () => {
  // An older build (from before `module_schema`) cannot know that a table has moved on, so it must
  // be able to go on inserting, updating, deleting and reading as it did. The module's migration is
  // checked for what could stop it: a unique index, a trigger, a column with a CHECK. Every case
  // here is a version 2 of module `a` on top of a version 1 that has the table `a_items`.
  const BASE = [
    "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0)",
  ];
  const objectNames = async (db: SqlDb) =>
    (
      await db.all<{ name: string }>(
        "SELECT name FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name",
      )
    ).map((r) => r.name);

  async function moveOn(
    statements: string[],
    more: { first?: string[]; options?: Partial<SchemaModule> } = {},
  ) {
    const first = more.first ?? BASE;
    const db = memoryDb();
    await ensureModules(db, [module("a", { ...more.options, steps: { 1: first } })]);
    const objects = await objectNames(db);
    const a2 = module("a", { ...more.options, steps: { 1: first, 2: statements } });
    const error = await ensureModules(db, [a2]).catch((e: unknown) => e);
    return { db, error, objects };
  }

  test.each<[string, string[], { first?: string[] }?]>([
    ["an index on one column", ["CREATE INDEX a_items_note ON a_items (note)"]],
    [
      "an index on two columns, one of them descending and one with a collation",
      ["CREATE INDEX a_items_two ON a_items (note COLLATE NOCASE, n DESC)"],
    ],
    [
      "a table of its own with a key, a UNIQUE and a CHECK",
      [
        "CREATE TABLE a_tags (id TEXT PRIMARY KEY NOT NULL, name TEXT UNIQUE, n INTEGER CHECK (n >= 0))",
      ],
    ],
    [
      "tables that refer to each other and to themselves",
      [
        "CREATE TABLE a_p (id TEXT PRIMARY KEY, up TEXT REFERENCES a_p(id))",
        "CREATE TABLE a_c (id TEXT, p TEXT REFERENCES a_p(id))",
      ],
    ],
    [
      "columns that may be empty, one with a sized type and one with two words",
      [
        "ALTER TABLE a_items ADD COLUMN s VARCHAR(10)",
        "ALTER TABLE a_items ADD COLUMN m DECIMAL(10, 2) DEFAULT -1.5",
        "ALTER TABLE a_items ADD COLUMN dp DOUBLE PRECISION",
        "ALTER TABLE a_items ADD COLUMN bare",
      ],
    ],
    [
      "columns with a quoted name and a default text that holds a comma and a bracket",
      [
        `ALTER TABLE a_items ADD COLUMN "two words" TEXT DEFAULT 'a, (b)'`,
        "ALTER TABLE a_items ADD COLUMN [bracketed] TEXT",
      ],
    ],
    [
      "columns that are NOT NULL with a default, and defaults that are NULL, TRUE, a blob or a number",
      [
        "ALTER TABLE a_items ADD COLUMN f INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE a_items ADD COLUMN g TEXT DEFAULT NULL",
        "ALTER TABLE a_items ADD COLUMN t INTEGER DEFAULT TRUE",
        "ALTER TABLE a_items ADD COLUMN bl BLOB DEFAULT x'00ff'",
        "ALTER TABLE a_items ADD COLUMN neg INTEGER NOT NULL DEFAULT -5",
        "ALTER TABLE a_items ADD COLUMN h INTEGER DEFAULT 0x10",
        "ALTER TABLE a_items ADD COLUMN r REAL DEFAULT 1e3",
        "ALTER TABLE a_items ADD COLUMN z TEXT NULL",
      ],
    ],
    [
      "columns whose names are keywords, written as quoted names",
      [
        `ALTER TABLE a_items ADD COLUMN "check" TEXT`,
        `ALTER TABLE a_items ADD COLUMN "default" TEXT`,
        `ALTER TABLE a_items ADD COLUMN "generated" TEXT`,
      ],
    ],
    [
      "a column and an index on a table and an index whose names have a double quote in them",
      [`ALTER TABLE "a_x""y" ADD COLUMN more TEXT`, `CREATE INDEX "a_j""z" ON "a_x""y" (more)`],
      {
        first: [
          `CREATE TABLE "a_x""y" (id TEXT PRIMARY KEY NOT NULL, note TEXT)`,
          `CREATE INDEX "a_i""x" ON "a_x""y" (note)`,
        ],
      },
    ],
    [
      "columns on a table with a key over two columns and WITHOUT ROWID",
      [
        "ALTER TABLE a_pairs ADD COLUMN z TEXT",
        "ALTER TABLE a_pairs ADD COLUMN w INTEGER NOT NULL DEFAULT 0",
      ],
      {
        first: [
          ...BASE,
          "CREATE TABLE a_pairs (x TEXT NOT NULL, y TEXT NOT NULL, PRIMARY KEY (x, y)) WITHOUT ROWID",
        ],
      },
    ],
  ])("may add %s", async (_, statements, more) => {
    const { db, error } = await moveOn(statements, more);
    expect(error, String(error)).toBeUndefined();
    expect((await versions(db)).a).toBe(2);
  });

  test.each<[string, string[], RegExp, { first?: string[]; options?: Partial<SchemaModule> }?]>([
    [
      "a unique index",
      ["CREATE UNIQUE INDEX a_items_note ON a_items (note)"],
      /made the unique index a_items_note/,
    ],
    [
      "a unique index on a table it makes in the same migration",
      ["CREATE TABLE a_tags (name TEXT)", "CREATE UNIQUE INDEX a_tags_name ON a_tags (name)"],
      /made the unique index a_tags_name/,
    ],
    [
      "a unique index that only covers some rows",
      ["CREATE UNIQUE INDEX a_items_open ON a_items (note) WHERE n = 0"],
      /made the unique index a_items_open/,
    ],
    [
      "an index that covers only some rows",
      ["CREATE INDEX a_items_open ON a_items (note) WHERE n = 0"],
      /made the index a_items_open with a WHERE clause/,
    ],
    [
      "an index on an expression",
      ["CREATE INDEX a_items_lower ON a_items (lower(note))"],
      /made the index a_items_lower on an expression/,
    ],
    [
      "an index that replaces one of the same name with a unique one",
      ["DROP INDEX a_items_note", "CREATE UNIQUE INDEX a_items_note ON a_items (note)"],
      /changed the unique index a_items_note/,
      { first: [...BASE, "CREATE INDEX a_items_note ON a_items (note)"] },
    ],
    [
      "a trigger",
      [
        "CREATE TRIGGER a_items_guard BEFORE INSERT ON a_items BEGIN SELECT RAISE(ABORT, 'no'); END",
      ],
      /made the trigger a_items_guard/,
    ],
    [
      "a trigger on a table it makes in the same migration",
      [
        "CREATE TABLE a_log (id TEXT)",
        "CREATE TRIGGER a_log_t AFTER INSERT ON a_log BEGIN SELECT 1; END",
      ],
      /made the trigger a_log_t/,
    ],
    ["a view", ["CREATE VIEW a_view AS SELECT id FROM a_items"], /made the view a_view/],
    [
      "a view with the name of a table of the first layout that is not in the file",
      ["CREATE VIEW old AS SELECT id FROM a_items"],
      /made the view old/,
      { options: { legacyTables: ["old"] } },
    ],
    [
      "a virtual table",
      ["CREATE VIRTUAL TABLE a_search USING fts5(text)"],
      /made the virtual table a_search/,
    ],
    [
      "a table with a foreign key to a table that was in the file",
      ["CREATE TABLE a_notes (id TEXT PRIMARY KEY, item TEXT REFERENCES a_items(id))"],
      /made the table a_notes with a foreign key to a_items/,
    ],
    [
      "a table with a foreign key to a table that was in the file, spelled in capitals",
      ["CREATE TABLE a_notes (id TEXT PRIMARY KEY, item TEXT REFERENCES A_ITEMS(id))"],
      /made the table a_notes with a foreign key to A_ITEMS/,
    ],
    [
      "a column with a CHECK",
      ["ALTER TABLE a_items ADD COLUMN chk INTEGER NOT NULL DEFAULT 1 CHECK (chk > 0)"],
      /added the column a_items\.chk with a CHECK constraint/,
    ],
    [
      "a column that refers to a table",
      ["ALTER TABLE a_items ADD COLUMN ref TEXT REFERENCES a_items(id)"],
      /added the column a_items\.ref with a foreign key/,
    ],
    [
      "a column with a collation",
      ["ALTER TABLE a_items ADD COLUMN tag TEXT COLLATE NOCASE"],
      /added the column a_items\.tag with a collation/,
    ],
    [
      "a generated column",
      ["ALTER TABLE a_items ADD COLUMN g TEXT GENERATED ALWAYS AS (note || 'x') VIRTUAL"],
      /added the column a_items\.g with a generated value/,
    ],
    [
      "a generated column without the words GENERATED ALWAYS",
      ["ALTER TABLE a_items ADD COLUMN g TEXT AS (note)"],
      /added the column a_items\.g with a generated value/,
    ],
    [
      "a column whose default is an expression",
      ["ALTER TABLE a_items ADD COLUMN d INTEGER DEFAULT (1 + 1)"],
      /added the column a_items\.d with a default that is not a plain value/,
    ],
    [
      "a column with a named constraint",
      ["ALTER TABLE a_items ADD COLUMN k TEXT CONSTRAINT k_nn NOT NULL DEFAULT 'x'"],
      /added the column a_items\.k with a named constraint/,
    ],
    [
      "a column with an ON CONFLICT clause",
      ["ALTER TABLE a_items ADD COLUMN k TEXT NOT NULL ON CONFLICT IGNORE DEFAULT 'x'"],
      /added the column a_items\.k with an ON CONFLICT clause/,
    ],
    [
      "a second column that is not plain, after one that is",
      [
        "ALTER TABLE a_items ADD COLUMN okay TEXT",
        "ALTER TABLE a_items ADD COLUMN bad INTEGER CHECK (bad > 0)",
      ],
      /added the column a_items\.bad with a CHECK constraint/,
    ],
    [
      "the table dropped and made again with a UNIQUE column added to its text",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, u TEXT UNIQUE)",
      ],
      /added the column a_items\.u with a UNIQUE constraint/,
    ],
    [
      "the table dropped and made again with a key column added to its text",
      ["DROP TABLE a_plain", "CREATE TABLE a_plain (note TEXT, k TEXT PRIMARY KEY)"],
      /added the column a_plain\.k with a primary key/,
      { first: [...BASE, "CREATE TABLE a_plain (note TEXT)"] },
    ],
    [
      "the table made again with the same columns and a CHECK",
      [
        "CREATE TABLE a_new (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, CHECK (n >= 0))",
        "DROP TABLE a_items",
        "ALTER TABLE a_new RENAME TO a_items",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with the same columns and a UNIQUE",
      [
        "CREATE TABLE a_new (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, UNIQUE (note))",
        "DROP TABLE a_items",
        "ALTER TABLE a_new RENAME TO a_items",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again exactly as it was",
      [
        "CREATE TABLE a_new (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0)",
        "DROP TABLE a_items",
        "ALTER TABLE a_new RENAME TO a_items",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with a table CHECK after the last column (it is not a column called CHECK)",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, CHECK (0))",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with a new column and a table CHECK after it",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0, more TEXT, CHECK (0))",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with a new column spliced into the definition of the last one, which takes its default",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL, extra INTEGER DEFAULT 0)",
      ],
      /changed the column a_items\.n;/,
    ],
    [
      "the table made again with a new column between the others",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, extra TEXT, note TEXT, n INTEGER NOT NULL DEFAULT 0)",
      ],
      /changed the column a_items\.note;/,
    ],
    [
      "the table made again with the columns in another order",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, n INTEGER NOT NULL DEFAULT 0, note TEXT)",
      ],
      /changed the column a_items\.note;/,
    ],
    [
      "the table made again with a UNIQUE on a kept column",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT UNIQUE, n INTEGER NOT NULL DEFAULT 0)",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with a collation on a kept column",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT COLLATE NOCASE, n INTEGER NOT NULL DEFAULT 0)",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with a CHECK on a kept column",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL, note TEXT, n INTEGER NOT NULL DEFAULT 0 CHECK (n >= 0))",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "the table made again with its text written in another way",
      [
        "DROP TABLE a_items",
        "CREATE TABLE a_items (id TEXT PRIMARY KEY NOT NULL,\n  note TEXT, n INTEGER NOT NULL DEFAULT 0)",
      ],
      /rebuilt the table a_items/,
    ],
    [
      "a column whose name is a word that starts a clause",
      ["ALTER TABLE a_items ADD COLUMN generated TEXT"],
      /added the column a_items\.generated with the word generated where the name of a column should be/,
    ],
    [
      "the removal of an index of its own",
      ["DROP INDEX a_items_note"],
      /removed the index a_items_note, which an older build still uses/,
      { first: [...BASE, "CREATE INDEX a_items_note ON a_items (note)"] },
    ],
    [
      "a unique index on a table whose name has a double quote in it",
      [`CREATE UNIQUE INDEX "a_u""x" ON "a_x""y" (note)`],
      /made the unique index a_u"x/,
      {
        first: [
          `CREATE TABLE "a_x""y" (id TEXT PRIMARY KEY NOT NULL, note TEXT)`,
          `CREATE INDEX "a_i""x" ON "a_x""y" (note)`,
        ],
      },
    ],
    [
      "a CHECK on a column added to a table whose name has a double quote in it",
      [`ALTER TABLE "a_x""y" ADD COLUMN chk INTEGER NOT NULL DEFAULT 1 CHECK (chk > 0)`],
      /added the column a_x"y\.chk with a CHECK constraint/,
      {
        first: [
          `CREATE TABLE "a_x""y" (id TEXT PRIMARY KEY NOT NULL, note TEXT)`,
          `CREATE INDEX "a_i""x" ON "a_x""y" (note)`,
        ],
      },
    ],
    ["the layout version", ["PRAGMA user_version = 7"], /changed PRAGMA user_version/],
  ])("may not add %s", async (_, statements, message, more) => {
    const { db, error, objects } = await moveOn(statements, more);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(message);
    // Taken back: the version is the old one and nothing is left of what the migration made.
    expect((await versions(db)).a).toBe(1);
    expect(await objectNames(db)).toEqual(objects);
    expect(await layout(db)).toBe(1);
  });

  describe("in a new file, where the tables are made for the first time", () => {
    // A file that this build makes is opened by the older build too, and that build runs
    // `CREATE TABLE IF NOT EXISTS` for the tables it knows and inserts into them.
    test.each([
      [
        "a unique index on a legacy table",
        ["CREATE UNIQUE INDEX trip_x ON trip (x)"],
        /unique index trip_x/,
      ],
      [
        "a trigger on a legacy table",
        ["CREATE TRIGGER trip_t BEFORE INSERT ON trip BEGIN SELECT 1; END"],
        /trigger trip_t/,
      ],
      [
        "a view with the name of a legacy table",
        ["CREATE VIEW point AS SELECT x FROM trip"],
        /view point/,
      ],
      ["the layout version", ["PRAGMA user_version = 7"], /changed PRAGMA user_version/],
    ])("may not make %s", async (_, statements, message) => {
      const db = memoryDb();
      const m = module("m", {
        legacyTables: ["trip", "point"],
        legacyDefinitions: { trip: "CREATE TABLE trip (id TEXT, x TEXT)" },
        steps: { 1: ["CREATE TABLE trip (id TEXT, x TEXT)", ...statements] },
      });
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(message);
      expect(await tables(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });
  });
});

describe("the reader of the columns that a migration added", () => {
  // `columnsProblem` reads the text that `ALTER TABLE ADD COLUMN` adds to a table's definition. A
  // few of the texts here are ones that SQLite would not accept in an `ALTER` (a key, a UNIQUE) or
  // not produce (an unbalanced bracket); the reader has to answer for any text, and refuse what it
  // does not understand.
  test.each([
    [", a"],
    [", a TEXT"],
    [", a TEXT, b INTEGER"],
    [", a VARCHAR(10)"],
    [", a DECIMAL(10, 2)"],
    [", a DECIMAL(+10,-2)"],
    [", a DOUBLE PRECISION"],
    [", a NOT NULL DEFAULT 0"],
    [", a TEXT DEFAULT 'it''s, (ok)'"],
    [`, "x y" TEXT, \`z\` TEXT, [w] TEXT`],
    [", a INT DEFAULT -1"],
    [", a REAL DEFAULT .5"],
    [", a REAL DEFAULT 1."],
    [", a REAL DEFAULT 1.5e-3"],
    [", a INT DEFAULT 0xFF"],
    [", a BLOB DEFAULT X'AB'"],
    [", a TEXT DEFAULT NULL"],
    [", a INT DEFAULT TRUE"],
    [", a INT DEFAULT false"],
    [", a TEXT DEFAULT CURRENT_TIMESTAMP"],
    [", a TEXT NULL"],
    [", a text not null default 0"],
    [", a TEXT /* CHECK (a > 0) */ NOT NULL DEFAULT 0 -- UNIQUE\n, b TEXT"],
    [", a TEXT, b INTEGER NOT NULL DEFAULT 0"],
    // A word that starts a clause is a name when it is quoted.
    [`, "check" TEXT, "default" TEXT, [generated] TEXT, \`unique\` TEXT`],
  ])("%s is plain", (added) => {
    expect(columnsProblem(added)).toBeNull();
  });

  // The words that start a constraint of a column or of a table, or a clause of a column, are not
  // names: `, CHECK (0)` is a constraint of the table, and read as a column called CHECK with a
  // size it let a rebuild of a table through that no column of the file showed.
  test.each([
    "CONSTRAINT",
    "PRIMARY",
    "UNIQUE",
    "CHECK",
    "FOREIGN",
    "GENERATED",
    "REFERENCES",
    "DEFAULT",
    "COLLATE",
    "NOT",
    "NULL",
    "AS",
    "ON",
    "check",
  ])("the word %s is not the name of a column", (word) => {
    for (const text of [`, ${word} TEXT`, `, ${word} (0)`, `, ok TEXT, ${word}`]) {
      const problem = columnsProblem(text);
      expect(problem, text).not.toBeNull();
      expect(problem?.why, text).toMatch(/where the name of a column should be/);
    }
  });

  test("a constraint of the table is not a column, whatever it looks like", () => {
    expect(columnsProblem(", CHECK (0)")).toEqual({
      column: "CHECK",
      why: "the word CHECK where the name of a column should be",
    });
    expect(columnsProblem(", PRIMARY KEY (a, b)")?.column).toBe("PRIMARY");
    expect(columnsProblem(", UNIQUE (a)")?.column).toBe("UNIQUE");
    expect(columnsProblem(", FOREIGN KEY (a) REFERENCES t(id)")?.column).toBe("FOREIGN");
    expect(columnsProblem(", CONSTRAINT c CHECK (a)")?.column).toBe("CONSTRAINT");
  });

  test.each([
    [", a TEXT FOREIGN KEY", /a foreign key/],
    [", a TEXT GENERATED", /a generated value/],
    [", a TEXT GENERATED ALWAYS", /a generated value/],
  ])("the word that starts a clause ends the type: %s", (text, why) => {
    expect(columnsProblem(text)?.why).toMatch(why);
  });

  test.each<[string, string, RegExp]>([
    ["", "?", /cannot be read/],
    ["a TEXT", "?", /cannot be read/],
    [", a TEXT )", "?", /cannot be read/],
    [", a VARCHAR(10", "?", /cannot be read/],
    [", a DEFAULT 'x", "?", /cannot be read/],
    [", a DEFAULT x'ab", "?", /cannot be read/],
    [", a /* open", "?", /cannot be read/],
    [', a TEXT DEFAULT "x', "?", /cannot be read/],
    [", a TEXT,", "?", /no name/],
    [", 5 TEXT", "5", /no name/],
    [", a VARCHAR(x)", "a", /type size/],
    [", a DECIMAL(1,)", "a", /type size/],
    [", a DECIMAL(1 2)", "a", /type size/],
    [", a NOT NULL", "a", /NOT NULL and no default/],
    [", a NOT NULL DEFAULT NULL", "a", /NOT NULL and no default/],
    [", a TEXT DEFAULT", "a", /default that is not a plain value/],
    [", a TEXT DEFAULT -", "a", /default that is not a plain value/],
    [", a TEXT DEFAULT -x", "a", /default that is not a plain value/],
    [", a TEXT DEFAULT (1)", "a", /default that is not a plain value/],
    [", a TEXT DEFAULT abc", "a", /default that is not a plain value/],
    [", a TEXT UNIQUE", "a", /a UNIQUE constraint/],
    [", a TEXT PRIMARY KEY", "a", /a primary key/],
    [", a TEXT CHECK (a > 0)", "a", /a CHECK constraint/],
    [", a TEXT REFERENCES t(id)", "a", /a foreign key/],
    [", a TEXT COLLATE NOCASE", "a", /a collation/],
    [", a TEXT GENERATED ALWAYS AS (1)", "a", /a generated value/],
    [", a TEXT AS (1)", "a", /a generated value/],
    [", a TEXT CONSTRAINT c NOT NULL DEFAULT 1", "a", /a named constraint/],
    [", a TEXT NOT NULL ON CONFLICT FAIL DEFAULT 1", "a", /an ON CONFLICT clause/],
    [", a FOO(1) BAR", "a", /the text BAR/],
    [', a TEXT "x"', "a", /the text x/],
    [", a TEXT, b TEXT CHECK (1)", "b", /a CHECK constraint/],
    [", a TEXT -- CHECK (x)\n, b TEXT UNIQUE", "b", /a UNIQUE constraint/],
  ])("%s is not: column %s, %s", (added, column, why) => {
    const problem = columnsProblem(added);
    expect(problem).not.toBeNull();
    expect(problem?.column).toBe(column);
    expect(problem?.why).toMatch(why);
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
      "a legacy meta key claimed twice",
      [module("a", { legacyMetaKeys: ["k"] }), module("b", { legacyMetaKeys: ["k"] })],
      /meta key k is claimed by a and by b/,
    ],
    [
      "a table of SQLite as a legacy table",
      [module("a", { legacyTables: ["sqlite_sequence"] })],
      /SQLite/,
    ],
    [
      "a definition for a table that is not one of its legacy tables",
      [
        module("a", {
          legacyTables: ["trip"],
          legacyDefinitions: { point: "CREATE TABLE point (id TEXT)" },
        }),
      ],
      /point has a legacy definition but is not in legacyTables/,
    ],
  ])("%s is refused before anything is written", async (_, modules, message) => {
    const db = memoryDb();
    const { spy, log } = watched(db);
    await expect(ensureModules(spy, modules)).rejects.toThrow(message);
    expect(log).toEqual([]);
  });
});

describe("a table of the first layout that a migration makes", () => {
  // An older build opens the tables of the first layout and writes to them. A migration that makes
  // one of them (the first one, in a new file) has to make exactly the table the module says that
  // the first layout had, or the older build is writing to a table it does not know.
  const DEFINITION = "CREATE TABLE IF NOT EXISTS trip (id TEXT PRIMARY KEY NOT NULL, x TEXT);";
  const makes = (statement: string, options: Partial<SchemaModule> = {}) =>
    module("m", {
      legacyTables: ["trip"],
      legacyDefinitions: { trip: DEFINITION },
      steps: { 1: [statement] },
      ...options,
    });

  test.each([
    [
      "written as the definition is",
      "CREATE TABLE IF NOT EXISTS trip (id TEXT PRIMARY KEY NOT NULL, x TEXT)",
    ],
    ["written without IF NOT EXISTS", "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x TEXT)"],
    [
      "written in small letters and over two lines",
      "create table\n  if not exists trip (id TEXT PRIMARY KEY NOT NULL, x TEXT);",
    ],
  ])("may be made %s", async (_, statement) => {
    const db = memoryDb();
    await ensureModules(db, [makes(statement)]);
    expect(await tables(db)).toEqual(["module_schema", "trip"]);
  });

  test.each([
    ["another type", "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x INTEGER)"],
    ["another column", "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x TEXT, y TEXT)"],
    ["another key", "CREATE TABLE trip (id TEXT NOT NULL, x TEXT)"],
    ["a constraint", "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x TEXT, CHECK (0))"],
    ["other spacing", "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL,x TEXT)"],
    [
      "another table option",
      "CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x TEXT) WITHOUT ROWID",
    ],
  ])("may not be made with %s", async (_, statement) => {
    const db = memoryDb();
    const error = await ensureModules(db, [makes(statement)]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(
      /^module m made the table trip, which an older build also makes and writes to; .*this is not it$/,
    );
    expect(await tables(db)).toEqual([]);
    expect(await layout(db)).toBe(0);
  });

  test("may not be made by a module that does not say what the table was", async () => {
    const db = memoryDb();
    const error = await ensureModules(db, [
      makes("CREATE TABLE trip (id TEXT PRIMARY KEY NOT NULL, x TEXT)", {
        legacyDefinitions: undefined,
      }),
    ]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(/the module declares none$/);
    expect(await tables(db)).toEqual([]);
  });

  test("may be found in a file, whatever the module says about it: that is not making it", async () => {
    // The file has the table already; `IF NOT EXISTS` does nothing, and the module did not make it.
    const db = memoryDb();
    await db.exec("CREATE TABLE trip (id TEXT); PRAGMA user_version = 1");
    await ensureModules(db, [
      module("m", {
        legacyTables: ["trip"],
        steps: { 1: ["CREATE TABLE IF NOT EXISTS trip (id TEXT, other TEXT)"] },
      }),
    ]);
    expect(await versions(db)).toEqual({ m: 1 });
    expect((await db.all<{ name: string }>("PRAGMA table_info(trip)")).map((c) => c.name)).toEqual([
      "id",
    ]);
  });

  test("a table whose name only looks like a legacy table is not one", async () => {
    const db = memoryDb();
    const error = await ensureModules(db, [
      module("m", {
        legacyTables: ["trip"],
        legacyDefinitions: { trip: DEFINITION },
        steps: { 1: ["CREATE TABLE trip_log (id TEXT)"] },
      }),
    ]).catch((e: unknown) => e);
    expect((error as Error).message).toMatch(/made the table trip_log .*does not own/);
  });
});

describe("the text that SQLite keeps for a table", () => {
  test.each([
    "CREATE TABLE IF NOT EXISTS t (a TEXT);",
    "create\n table if not exists t (a TEXT)",
    "CREATE TABLE t (\n  a TEXT PRIMARY KEY,\n  b TEXT\n) WITHOUT ROWID;\n",
    "  CREATE  TABLE  IF  NOT  EXISTS  t (a TEXT)  ",
    "CREATE TABLE t (a TEXT);  ",
  ])("is what `storedDefinition` makes of %j", async (statement) => {
    const db = memoryDb();
    await db.exec(statement);
    const stored = await db.one<{ sql: string }>("SELECT sql FROM sqlite_master WHERE name = 't'");
    expect(storedDefinition(statement)).toBe(stored?.sql);
  });

  test("a definition in the middle of a text is left as it is", () => {
    expect(storedDefinition("CREATE TABLE IF NOT EXISTS t (a TEXT DEFAULT 'IF NOT EXISTS')")).toBe(
      "CREATE TABLE t (a TEXT DEFAULT 'IF NOT EXISTS')",
    );
    expect(storedDefinition("CREATE TABLE t (a TEXT);")).toBe("CREATE TABLE t (a TEXT)");
  });
});

describe("the pieces that a migration added to a table's text", () => {
  // `appended(was, now, kept, added)`: what `now` has and `was` has not, when `now` is `was` with
  // `added` definitions put in after the first `kept` pieces and nothing else is different.
  const PLAIN = "CREATE TABLE t (a TEXT, b TEXT)";

  test.each<[string, string, string, number, number, string[]]>([
    ["one column at the end", PLAIN, "CREATE TABLE t (a TEXT, b TEXT, c TEXT)", 2, 1, [" c TEXT"]],
    [
      "two columns",
      PLAIN,
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT, d INTEGER DEFAULT 0)",
      2,
      2,
      [" c TEXT", " d INTEGER DEFAULT 0"],
    ],
    [
      "a column where ALTER TABLE puts it: before the constraints of the table",
      "CREATE TABLE t (a TEXT, b TEXT,\n  PRIMARY KEY (a, b)\n) WITHOUT ROWID",
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT,\n  PRIMARY KEY (a, b)\n) WITHOUT ROWID",
      2,
      1,
      [" c TEXT"],
    ],
    [
      "a column where ALTER TABLE puts it when the last piece ends in a line break",
      "CREATE TABLE t (\n  a TEXT,\n  b TEXT\n)",
      "CREATE TABLE t (\n  a TEXT,\n  b TEXT\n, c TEXT)",
      2,
      1,
      [" c TEXT"],
    ],
    [
      "a column whose default has a comma and a bracket in it, and pieces with comments",
      "CREATE TABLE t (a TEXT DEFAULT 'x, (y', -- one, two\n b /* ), */ TEXT)",
      "CREATE TABLE t (a TEXT DEFAULT 'x, (y', -- one, two\n b /* ), */ TEXT, c TEXT DEFAULT ')')",
      2,
      1,
      [" c TEXT DEFAULT ')'"],
    ],
    ["nothing added to a table that is the same", PLAIN, PLAIN, 2, 0, []],
  ])("finds %s", (_, was, now, kept, added, expected) => {
    expect(appended(was, now, kept, added)).toEqual(expected);
  });

  test.each<[string, string, string, number, number]>([
    ["a piece changed", PLAIN, "CREATE TABLE t (a TEXT, b INTEGER, c TEXT)", 2, 1],
    ["a piece changed only by a space", PLAIN, "CREATE TABLE t (a TEXT,  b TEXT, c TEXT)", 2, 1],
    ["a column put in the middle", PLAIN, "CREATE TABLE t (a TEXT, c TEXT, b TEXT)", 2, 1],
    ["a column put first", PLAIN, "CREATE TABLE t (c TEXT, a TEXT, b TEXT)", 2, 1],
    ["the columns in another order", PLAIN, "CREATE TABLE t (b TEXT, a TEXT)", 2, 0],
    [
      "a constraint put after the new column when none was there",
      PLAIN,
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT, CHECK (0))",
      2,
      1,
    ],
    ["a constraint added and no column", PLAIN, "CREATE TABLE t (a TEXT, b TEXT, CHECK (0))", 2, 0],
    [
      "a piece too many for the columns that were added",
      PLAIN,
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT, d TEXT)",
      2,
      1,
    ],
    [
      "a piece too few for the columns that were added",
      PLAIN,
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT)",
      2,
      2,
    ],
    ["a piece removed", PLAIN, "CREATE TABLE t (a TEXT)", 2, 0],
    ["another name", PLAIN, "CREATE TABLE u (a TEXT, b TEXT, c TEXT)", 2, 1],
    ["another beginning", PLAIN, "CREATE TEMP TABLE t (a TEXT, b TEXT, c TEXT)", 2, 1],
    [
      "another end",
      "CREATE TABLE t (a TEXT, b TEXT) WITHOUT ROWID",
      "CREATE TABLE t (a TEXT, b TEXT, c TEXT)",
      2,
      1,
    ],
    ["an end added", PLAIN, "CREATE TABLE t (a TEXT, b TEXT, c TEXT) STRICT", 2, 1],
    ["an old text without parentheses", "CREATE TABLE t", "CREATE TABLE t (a TEXT)", 0, 1],
    ["a new text without parentheses", PLAIN, "CREATE TABLE t", 2, 0],
    [
      "an old text that closes before it opens",
      "CREATE TABLE t ) (a TEXT",
      "CREATE TABLE t ) (a TEXT, b TEXT",
      1,
      1,
    ],
    [
      "an old text that does not close",
      "CREATE TABLE t (a TEXT",
      "CREATE TABLE t (a TEXT, b TEXT",
      1,
      1,
    ],
    ["a new text that does not close", PLAIN, "CREATE TABLE t (a TEXT, b TEXT, c TEXT", 2, 1],
    [
      "a text with a string that does not end",
      "CREATE TABLE t (a TEXT DEFAULT 'x)",
      "CREATE TABLE t (a TEXT DEFAULT 'x), b TEXT)",
      1,
      1,
    ],
    [
      "a text with a comment that does not end",
      "CREATE TABLE t (a TEXT /* x)",
      "CREATE TABLE t (a TEXT /* x), b TEXT)",
      1,
      1,
    ],
  ])("finds nothing when there is %s", (_, was, now, kept, added) => {
    expect(appended(was, now, kept, added)).toBeNull();
  });

  test("reads the first group of parentheses only, and nothing that closes before it opens", () => {
    expect(
      appended("CREATE TABLE t (a TEXT) (x, y)", "CREATE TABLE t (a TEXT, b TEXT) (x, y)", 1, 1),
    ).toEqual([" b TEXT"]);
    // A closing parenthesis with no opening one is not the beginning of a group: the text is unreadable.
    expect(
      appended("CREATE TABLE t ) ((a TEXT)", "CREATE TABLE t ) ((a TEXT, b TEXT)", 1, 1),
    ).toBeNull();
  });

  test("looks for the new pieces after the columns that were kept, not at the end", () => {
    // Five pieces now, three before: the two new ones are after the first two, whatever comes next.
    const was = "CREATE TABLE t (a, b, PRIMARY KEY (a))";
    const now = "CREATE TABLE t (a, b, c, d, PRIMARY KEY (a))";
    expect(appended(was, now, 2, 2)).toEqual([" c", " d"]);
    expect(appended(was, now, 3, 2)).toBeNull();
    expect(appended(was, now, 1, 2)).toBeNull();
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

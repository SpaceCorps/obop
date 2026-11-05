import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "./host-state.ts";
import { SchemaError, ensureModules, type SchemaModule } from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { MILEAGE_SCHEMA, migrate } from "./sql-store.ts";
import { emptyLegacyFile, legacyFile } from "../test-support/legacy-file.ts";
import { SCHEMA as LEGACY_SCHEMA } from "../test-support/legacy-schema.ts";
import { memoryDb } from "../test-support/memory-db.ts";
import { probeOlderBuild } from "../test-support/older-build-probe.ts";

// The check that the schema manager makes of a module's migration (`checkChange`) is a lint. This
// file is where it meets the oracle: the first build's own statements, run against the file by
// `probeOlderBuild`. The probe is tested first, then every shape of migration that the tests use
// is applied twice: by hand to a file of the first build, to see whether the first build still
// works on it, and through the manager, to see whether it lets the migration through.
//
// Every shape is in one of three groups, and the group is checked, not assumed:
//   - accepted: the manager lets it through and the probe passes on the file it leaves;
//   - refused, and the first build breaks: made by hand, the probe fails at the named step. These are
//     the refusals the oracle confirms;
//   - refused, and the probe passes: the lint is stricter than the oracle can show. Each carries
//     the reason it is refused anyway. A shape does not get into this group by accident: it has to
//     be one that the probe really does pass.

const objectsOf = (db: SqlDb) =>
  db.all<{ type: string; name: string; sql: string | null }>(
    "SELECT type, name, sql FROM sqlite_master ORDER BY type, name",
  );
const layout = async (db: SqlDb) =>
  Number((await db.one<{ user_version: number }>("PRAGMA user_version"))?.user_version);
const rowsOf = async (db: SqlDb) => ({
  trip: await db.all("SELECT * FROM trip ORDER BY client_trip_id"),
  point: await db.all("SELECT * FROM point ORDER BY client_trip_id, seq"),
  batch: await db.all("SELECT * FROM batch ORDER BY client_trip_id, first_seq"),
  meta: await db.all("SELECT * FROM meta ORDER BY key"),
});

/** The statement that made a table of the first layout, as it is kept in the file. */
function ddl(name: string): string {
  const found = new RegExp(
    `CREATE TABLE IF NOT EXISTS ${name} \\([\\s\\S]*?\\n\\)( WITHOUT ROWID)?;`,
  ).exec(LEGACY_SCHEMA);
  if (found === null) throw new Error(`the first layout has no table ${name}`);
  return found[0].replace("IF NOT EXISTS ", "").replace(/;$/, "");
}

/** Makes a table of the first layout again with one piece of its definition changed. */
function rebuilt(name: string, from: string | RegExp, to: string): string[] {
  const sql = ddl(name);
  const next = sql.replace(from, to);
  if (next === sql) throw new Error(`${String(from)} is not in the definition of ${name}`);
  return [`DROP TABLE ${name}`, next];
}

/** The outbox's module at version 2, with the migration that is being tried. */
function mileageAt2(statements: readonly string[]): SchemaModule {
  return {
    ...MILEAGE_SCHEMA,
    version: 2,
    async migrate(tx, from) {
      await MILEAGE_SCHEMA.migrate(tx, from);
      if (from < 2) for (const statement of statements) await tx.exec(statement);
    },
  };
}

async function byHand(statements: readonly string[]): Promise<SqlDb> {
  const db = await emptyLegacyFile();
  for (const statement of statements) await db.exec(statement);
  return db;
}

describe("the older-build probe", () => {
  test("passes on a file of the first build, with no rows and with rows", async () => {
    expect(await probeOlderBuild(await emptyLegacyFile())).toEqual({ ok: true });
    expect(await probeOlderBuild(await legacyFile())).toEqual({ ok: true });
  });

  test("passes on a file that this build made", async () => {
    const db = memoryDb();
    await migrate(db);
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
  });

  test("leaves the file as it was: its definitions, its rows and its layout", async () => {
    const db = await legacyFile();
    const before = { objects: await objectsOf(db), rows: await rowsOf(db) };
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
    expect(await objectsOf(db)).toEqual(before.objects);
    expect(await rowsOf(db)).toEqual(before.rows);
    expect(await layout(db)).toBe(1);
    // And again: nothing that it did is left behind to trip it the second time.
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
  });

  test("leaves the file as it was when it fails, too", async () => {
    const db = await legacyFile();
    await db.exec("CREATE TRIGGER x BEFORE UPDATE ON trip BEGIN SELECT RAISE(ABORT, 'no'); END");
    const before = { objects: await objectsOf(db), rows: await rowsOf(db) };
    const result = await probeOlderBuild(db);
    expect(result.ok).toBe(false);
    expect(await objectsOf(db)).toEqual(before.objects);
    expect(await rowsOf(db)).toEqual(before.rows);
  });

  test("does not open a file of a newer layout: the first build's own migration refuses it", async () => {
    const db = await emptyLegacyFile();
    await db.exec("PRAGMA user_version = 2");
    const result = await probeOlderBuild(db);
    expect(result).toMatchObject({ ok: false, step: "open the file" });
    expect((result as { error: string }).error).toMatch(/newer version of the app \(schema 2\)/);
    expect(await layout(db)).toBe(2);
  });

  test("opens a file with no layout the way the first build does, as a new one, and leaves it with no layout", async () => {
    const db = await emptyLegacyFile();
    await db.exec("PRAGMA user_version = 0");
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
    expect(await layout(db)).toBe(0);
  });

  // Each step of the probe is there because a file can be made that fails only at that step. A step
  // that nothing could fail at would be a line that tests nothing.
  test.each<[string, string[], RegExp]>([
    [
      "a unique index on the purpose of a trip",
      ["CREATE UNIQUE INDEX x ON trip (purpose)"],
      /^start a second trip/,
    ],
    [
      "a trigger that refuses an insert of a trip",
      ["CREATE TRIGGER x BEFORE INSERT ON trip BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^start a trip/,
    ],
    [
      "a table that is not there for a trip to be recorded twice",
      rebuilt("trip", "client_trip_id TEXT PRIMARY KEY NOT NULL", "client_trip_id TEXT NOT NULL"),
      /^refuse a trip that is recorded twice/,
    ],
    [
      "a unique index on the time of a fix",
      ["CREATE UNIQUE INDEX x ON point (t)"],
      /^record fixes/,
    ],
    [
      "fixes whose coordinates come back as numbers",
      rebuilt("point", "lat TEXT NOT NULL", "lat REAL NOT NULL"),
      /^record fixes/,
    ],
    [
      "fixes that can be recorded twice",
      rebuilt("point", ",\n  PRIMARY KEY (client_trip_id, seq)\n) WITHOUT ROWID", "\n)"),
      /^record fixes/,
    ],
    [
      "a trigger that refuses an update of a trip",
      ["CREATE TRIGGER x BEFORE UPDATE ON trip BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^stop a trip/,
    ],
    [
      "a trigger that refuses an update of a batch",
      ["CREATE TRIGGER x BEFORE UPDATE ON batch BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^cut and send batches/,
    ],
    [
      "a trigger that refuses the discarding of a trip",
      [
        "CREATE TRIGGER x BEFORE UPDATE OF discard_reason ON trip BEGIN SELECT RAISE(ABORT, 'no'); END",
      ],
      /^submit, discard/,
    ],
    [
      "a trigger that refuses the patching of the integrity of a trip",
      ["CREATE TRIGGER x BEFORE UPDATE OF integrity ON trip BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^patch a trip/,
    ],
    [
      "a trigger that refuses a trip that comes from the server",
      [
        "CREATE TRIGGER x BEFORE INSERT ON trip WHEN NEW.origin = 'server' BEGIN SELECT RAISE(ABORT, 'no'); END",
      ],
      /^merge what the server says/,
    ],
    [
      "a trigger that refuses a second update of a note",
      ["CREATE TRIGGER x BEFORE UPDATE ON meta BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^keep a note/,
    ],
    [
      "a trigger that refuses the deleting of a trip",
      ["CREATE TRIGGER x BEFORE DELETE ON trip BEGIN SELECT RAISE(ABORT, 'no'); END"],
      /^delete a trip/,
    ],
    [
      "a trigger that refuses the deleting of the batches of one trip",
      [
        "CREATE TRIGGER x BEFORE DELETE ON batch WHEN OLD.client_trip_id = 'probe-b' BEGIN SELECT RAISE(ABORT, 'no'); END",
      ],
      /^clear everything/,
    ],
    [
      "batches that can be recorded twice",
      rebuilt("batch", ",\n  PRIMARY KEY (client_trip_id, first_seq)\n) WITHOUT ROWID", "\n)"),
      /^cut and send batches/,
    ],
    [
      "a trigger that refuses the deleting of the batches of the first trip",
      [
        "CREATE TRIGGER x BEFORE DELETE ON batch WHEN OLD.client_trip_id = 'probe-a' BEGIN SELECT RAISE(ABORT, 'no'); END",
      ],
      /^delete a trip/,
    ],
    [
      "the last column of the trips gone",
      ["ALTER TABLE trip DROP COLUMN created_ms"],
      /^start a trip/,
    ],
    ["a layout that the first build refuses", ["PRAGMA user_version = 7"], /^open the file/],
  ])("fails at the right step for %s", async (_, statements, step) => {
    const result = await probeOlderBuild(await byHand(statements));
    expect(result).toMatchObject({ ok: false });
    expect((result as { step: string }).step).toMatch(step);
  });

  test("passes when a table that the first build never opens is added", async () => {
    const db = await byHand([
      "CREATE TABLE mileage_stop (id TEXT PRIMARY KEY NOT NULL, n INTEGER CHECK (n >= 0))",
      "CREATE UNIQUE INDEX mileage_stop_n ON mileage_stop (n)",
    ]);
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
  });
});

type Accepted = [name: string, statements: string[]];
type Breaks = [name: string, statements: string[], message: RegExp, step: RegExp];
type LintOnly = [name: string, statements: string[], message: RegExp, why: string];

const ACCEPTED: Accepted[] = [
  ["a column that may be empty on the trips", ["ALTER TABLE trip ADD COLUMN note TEXT"]],
  [
    "a NOT NULL column with a default on the trips",
    ["ALTER TABLE trip ADD COLUMN flag INTEGER NOT NULL DEFAULT 0"],
  ],
  [
    "columns with a text, a blob and a negative number as default",
    [
      "ALTER TABLE trip ADD COLUMN s TEXT DEFAULT 'a, (b)'",
      "ALTER TABLE trip ADD COLUMN bl BLOB DEFAULT x'00ff'",
      "ALTER TABLE trip ADD COLUMN neg REAL NOT NULL DEFAULT -1.5",
    ],
  ],
  [
    "columns with a sized type, a type of two words and no type",
    [
      "ALTER TABLE trip ADD COLUMN sz VARCHAR(10)",
      "ALTER TABLE trip ADD COLUMN m DECIMAL(10, 2) DEFAULT -1.5",
      "ALTER TABLE trip ADD COLUMN dp DOUBLE PRECISION",
      "ALTER TABLE trip ADD COLUMN bare",
    ],
  ],
  [
    "columns with a quoted name, also one that is a keyword, and a bracketed name",
    [
      `ALTER TABLE trip ADD COLUMN "two words" TEXT DEFAULT 'a, (b)'`,
      `ALTER TABLE trip ADD COLUMN "check" TEXT`,
      "ALTER TABLE trip ADD COLUMN [bracketed] TEXT",
    ],
  ],
  [
    "columns that are NOT NULL with a default, and defaults that are NULL, TRUE, a number in hex or an exponent",
    [
      "ALTER TABLE trip ADD COLUMN f INTEGER NOT NULL DEFAULT 0",
      "ALTER TABLE trip ADD COLUMN g TEXT DEFAULT NULL",
      "ALTER TABLE trip ADD COLUMN t INTEGER DEFAULT TRUE",
      "ALTER TABLE trip ADD COLUMN h INTEGER DEFAULT 0x10",
      "ALTER TABLE trip ADD COLUMN r REAL DEFAULT 1e3",
      "ALTER TABLE trip ADD COLUMN z TEXT NULL",
    ],
  ],
  [
    "columns on the fixes, which have a key over two columns and no rowid",
    [
      "ALTER TABLE point ADD COLUMN source TEXT",
      "ALTER TABLE point ADD COLUMN quality INTEGER NOT NULL DEFAULT 0",
    ],
  ],
  ["a column on the batches", ["ALTER TABLE batch ADD COLUMN note TEXT"]],
  [
    "indexes on the trips and on the fixes",
    [
      "CREATE INDEX mileage_trip_started ON trip (started_at)",
      "CREATE INDEX mileage_point_t ON point (client_trip_id, t DESC)",
    ],
  ],
  [
    "an index on two columns, one with a collation and one descending",
    ["CREATE INDEX mileage_trip_two ON trip (purpose COLLATE NOCASE, created_ms DESC)"],
  ],
  [
    "a table of its own with a key, a UNIQUE and a CHECK",
    [
      "CREATE TABLE mileage_stop (id TEXT PRIMARY KEY NOT NULL, name TEXT UNIQUE, n INTEGER CHECK (n >= 0))",
    ],
  ],
  [
    "tables of its own that refer to each other and to themselves",
    [
      "CREATE TABLE mileage_p (id TEXT PRIMARY KEY, up TEXT REFERENCES mileage_p(id))",
      "CREATE TABLE mileage_c (id TEXT, p TEXT REFERENCES mileage_p(id))",
    ],
  ],
  [
    "the trips made again with a column added where ADD COLUMN puts it",
    rebuilt("trip", /\n\)$/, "\n, extra TEXT)"),
  ],
];

const BREAKS: Breaks[] = [
  [
    "a unique index on the trips",
    ["CREATE UNIQUE INDEX mileage_trip_purpose ON trip (purpose)"],
    /made the unique index mileage_trip_purpose/,
    /^start a second trip/,
  ],
  [
    "a unique index on the fixes",
    ["CREATE UNIQUE INDEX mileage_point_t ON point (t)"],
    /made the unique index mileage_point_t/,
    /^record fixes/,
  ],
  [
    "a trigger on the trips",
    [
      "CREATE TRIGGER mileage_trip_guard BEFORE INSERT ON trip BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_trip_guard/,
    /^start a trip/,
  ],
  [
    "a trigger on the batches",
    [
      "CREATE TRIGGER mileage_batch_guard BEFORE UPDATE ON batch BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_batch_guard/,
    /^cut and send batches/,
  ],
  [
    "a trigger on the notes of the host",
    [
      "CREATE TRIGGER mileage_meta_guard BEFORE UPDATE ON meta BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_meta_guard on meta, which it does not own/,
    /^keep a note/,
  ],
  [
    "a column with a CHECK that the default does not satisfy",
    ["ALTER TABLE trip ADD COLUMN chk INTEGER NOT NULL DEFAULT 0 CHECK (chk > 0)"],
    /added the column trip\.chk with a CHECK constraint/,
    /^start a trip/,
  ],
  [
    "the trips made again with a table CHECK that nothing satisfies, after the last column (a column called CHECK to a naive reader)",
    rebuilt("trip", /\n\)$/, ",\n  CHECK (0)\n)"),
    /rebuilt the table trip/,
    /^start a trip/,
  ],
  [
    "the trips made again with a column spliced into the definition of the origin, so that its default is the new column's",
    rebuilt(
      "trip",
      "origin TEXT NOT NULL DEFAULT 'device'",
      "origin TEXT NOT NULL, extra TEXT DEFAULT 'device'",
    ),
    /changed the column trip\.origin/,
    /^start a trip/,
  ],
  [
    "the trips made again with a kept column that is NOT NULL",
    rebuilt("trip", "server_id TEXT,", "server_id TEXT NOT NULL,"),
    /changed the column trip\.server_id/,
    /^start a trip/,
  ],
  [
    "the trips made again with a kept column of another type, so that its values come back as text",
    rebuilt("trip", "created_ms INTEGER NOT NULL", "created_ms TEXT NOT NULL"),
    /changed the column trip\.created_ms/,
    /^start a trip/,
  ],
  [
    "the trips made again with another default for the origin",
    rebuilt("trip", "DEFAULT 'device'", "DEFAULT 'server'"),
    /changed the column trip\.origin/,
    /^start a trip/,
  ],
  [
    "the trips made again with a kept column that is a generated one",
    rebuilt("trip", "ended_at TEXT,", "ended_at TEXT GENERATED ALWAYS AS (NULL) VIRTUAL,"),
    /changed the column trip\.ended_at/,
    /^stop a trip/,
  ],
  [
    "the trips made again without their key",
    rebuilt("trip", "client_trip_id TEXT PRIMARY KEY NOT NULL", "client_trip_id TEXT NOT NULL"),
    /changed the column trip\.client_trip_id/,
    /^refuse a trip that is recorded twice/,
  ],
  [
    "the fixes made again without their key and as a table with a rowid",
    rebuilt("point", ",\n  PRIMARY KEY (client_trip_id, seq)\n) WITHOUT ROWID", "\n)"),
    /changed the column point\.client_trip_id/,
    /^record fixes/,
  ],
  [
    "the fixes made again with real coordinates",
    rebuilt("point", "lat TEXT NOT NULL", "lat REAL NOT NULL"),
    /changed the column point\.lat/,
    /^record fixes/,
  ],
  [
    "the trips made again with a UNIQUE on a kept column, which the columns of the file do not show",
    rebuilt("trip", "purpose TEXT NOT NULL,", "purpose TEXT NOT NULL UNIQUE,"),
    /rebuilt the table trip/,
    /^start a second trip/,
  ],
  [
    "the trips made again with a CHECK on a kept column, which the columns of the file do not show",
    rebuilt(
      "trip",
      "attempts INTEGER NOT NULL DEFAULT 0",
      "attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts > 5)",
    ),
    /rebuilt the table trip/,
    /^start a trip/,
  ],
  [
    "the trips made again with a NOT NULL column that has no default",
    rebuilt("trip", /\n\)$/, "\n, extra TEXT NOT NULL)"),
    /added the column trip\.extra as NOT NULL without a default/,
    /^start a trip/,
  ],
  [
    "a renamed column",
    ["ALTER TABLE trip RENAME COLUMN purpose TO reason"],
    /changed the column trip\.purpose/,
    /^start a trip/,
  ],
  [
    "a column renamed to the same name in capitals",
    ["ALTER TABLE trip RENAME COLUMN purpose TO Purpose"],
    /changed the column trip\.purpose/,
    /^start a trip/,
  ],
  [
    "a dropped column",
    ["ALTER TABLE trip DROP COLUMN problem_message"],
    /changed the column trip\.problem_message/,
    /^start a trip/,
  ],
  [
    "a dropped last column",
    ["ALTER TABLE trip DROP COLUMN created_ms"],
    /changed the column trip\.created_ms/,
    /^start a trip/,
  ],
  [
    "the batches made again without their key and as a table with a rowid",
    rebuilt("batch", ",\n  PRIMARY KEY (client_trip_id, first_seq)\n) WITHOUT ROWID", "\n)"),
    /changed the column batch\.client_trip_id/,
    /^cut and send batches/,
  ],
  [
    "a trigger on the deleting of the batches of the first trip",
    [
      "CREATE TRIGGER mileage_batch_first BEFORE DELETE ON batch WHEN OLD.client_trip_id = 'probe-a' BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_batch_first/,
    /^delete a trip/,
  ],
  ["a dropped table", ["DROP TABLE batch"], /removed the table batch/, /^cut and send batches/],
  ["a renamed table", ["ALTER TABLE batch RENAME TO batch_old"], /batch/, /^cut and send batches/],
  [
    "a view in the place of a table",
    [
      "DROP TABLE batch",
      "CREATE VIEW batch AS SELECT client_trip_id, 1 AS first_seq, 1 AS last_seq, 0 AS sent FROM trip",
    ],
    /removed the table batch/,
    /^cut and send batches/,
  ],
  [
    "a trigger on the deleting of trips",
    ["CREATE TRIGGER mileage_trip_gone BEFORE DELETE ON trip BEGIN SELECT RAISE(ABORT, 'no'); END"],
    /made the trigger mileage_trip_gone/,
    /^delete a trip/,
  ],
  [
    "a trigger on one update of the trips",
    [
      "CREATE TRIGGER mileage_trip_discard BEFORE UPDATE OF discard_reason ON trip BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_trip_discard/,
    /^submit, discard/,
  ],
  [
    "a trigger on the patching of the integrity",
    [
      "CREATE TRIGGER mileage_trip_integrity BEFORE UPDATE OF integrity ON trip BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_trip_integrity/,
    /^patch a trip/,
  ],
  [
    "a trigger on trips that come from the server",
    [
      "CREATE TRIGGER mileage_trip_server BEFORE INSERT ON trip WHEN NEW.origin = 'server' BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_trip_server/,
    /^merge what the server says/,
  ],
  [
    "a trigger on the deleting of the batches of one trip",
    [
      "CREATE TRIGGER mileage_batch_gone BEFORE DELETE ON batch WHEN OLD.client_trip_id = 'probe-b' BEGIN SELECT RAISE(ABORT, 'no'); END",
    ],
    /made the trigger mileage_batch_gone/,
    /^clear everything/,
  ],
  [
    "the layout version",
    ["PRAGMA user_version = 7"],
    /changed PRAGMA user_version/,
    /^open the file/,
  ],
];

const LINT_ONLY: LintOnly[] = [
  [
    "a view",
    ["CREATE VIEW mileage_open AS SELECT client_trip_id FROM trip"],
    /made the view mileage_open/,
    "a view only matters to the first build where a table of the first layout is missing, and none is",
  ],
  [
    "a virtual table",
    ["CREATE VIRTUAL TABLE mileage_search USING fts5(text)"],
    /made the virtual table mileage_search/,
    "the SQLite of the tests has the module; a phone's may not, and the probe cannot know",
  ],
  [
    "a table with a foreign key to the trips",
    ["CREATE TABLE mileage_note (id TEXT PRIMARY KEY, trip TEXT REFERENCES trip(client_trip_id))"],
    /made the table mileage_note with a foreign key to trip/,
    "it matters once the file has rows in the new table and foreign keys are on; the probe has no such rows",
  ],
  [
    "an index with a WHERE clause",
    ["CREATE INDEX mileage_open ON trip (started_at) WHERE ended_at IS NULL"],
    /made the index mileage_open with a WHERE clause/,
    "this one has no function in it; the lint cannot tell it from one that raises an error on a row",
  ],
  [
    "an index on an expression",
    ["CREATE INDEX mileage_lower ON trip (lower(purpose))"],
    /made the index mileage_lower on an expression/,
    "lower() is safe on every row; the lint cannot tell it from json_extract() on a row that is not JSON",
  ],
  [
    "a unique index on a table that the first build never opens",
    [
      "CREATE TABLE mileage_tag (name TEXT)",
      "CREATE UNIQUE INDEX mileage_tag_name ON mileage_tag (name)",
    ],
    /made the unique index mileage_tag_name/,
    "the first build writes nothing there; write UNIQUE inside the CREATE TABLE, which is allowed",
  ],
  [
    "a column with a CHECK that its default satisfies",
    ["ALTER TABLE trip ADD COLUMN chk INTEGER NOT NULL DEFAULT 1 CHECK (chk > 0)"],
    /added the column trip\.chk with a CHECK constraint/,
    "the first build never sets the column, so the default is all it ever gets",
  ],
  [
    "a column that refers to a table",
    ["ALTER TABLE trip ADD COLUMN parent TEXT REFERENCES trip(client_trip_id)"],
    /added the column trip\.parent with a foreign key/,
    "empty everywhere, and the first build does not turn foreign keys on",
  ],
  [
    "a column with a collation",
    ["ALTER TABLE trip ADD COLUMN tag TEXT COLLATE NOCASE"],
    /added the column trip\.tag with a collation/,
    "the first build does not compare it",
  ],
  [
    "a generated column",
    ["ALTER TABLE trip ADD COLUMN g TEXT GENERATED ALWAYS AS (purpose) VIRTUAL"],
    /added the column trip\.g with a generated value/,
    "the first build never writes it; a generated value that could raise an error is not told from one that cannot",
  ],
  [
    "a column whose default is an expression",
    ["ALTER TABLE trip ADD COLUMN d INTEGER DEFAULT (1 + 1)"],
    /added the column trip\.d with a default that is not a plain value/,
    "this expression is a constant; the lint cannot tell it from one that is not",
  ],
  [
    "a column with a named constraint",
    ["ALTER TABLE trip ADD COLUMN k TEXT CONSTRAINT k_nn NOT NULL DEFAULT 'x'"],
    /added the column trip\.k with a named constraint/,
    "NOT NULL with a default is fine for the first build; a constraint with a name is not told from a CHECK",
  ],
  [
    "a column called after a word that starts a clause",
    ["ALTER TABLE trip ADD COLUMN generated TEXT"],
    /added the column trip\.generated with the word generated where the name of a column should be/,
    "a legal name; refused because a reader that takes a keyword for a name has been wrong before (quote it to use it)",
  ],
  [
    "a kept column that is no longer NOT NULL",
    rebuilt("trip", "purpose TEXT NOT NULL,", "purpose TEXT,"),
    /changed the column trip\.purpose/,
    "the first build always writes the purpose; a column that may be empty reads the same",
  ],
  [
    "a kept column of another type whose affinity keeps the values",
    rebuilt("trip", "purpose TEXT NOT NULL,", "purpose INTEGER NOT NULL,"),
    /changed the column trip\.purpose/,
    "the first build writes text, and text stays text in a column of that affinity",
  ],
  [
    "the fixes made again as a table with a rowid and the same key",
    rebuilt("point", " WITHOUT ROWID", ""),
    /rebuilt the table point/,
    "the same statements give the same answers; only the way the file stores the table differs",
  ],
  [
    "the trips made again with a column added, written in a tidier way than ADD COLUMN writes it",
    rebuilt("trip", /\n\)$/, ",\n  extra TEXT\n)"),
    /rebuilt the table trip/,
    "the same table; the text is compared letter for letter, so that a piece of it cannot hide",
  ],
];

describe("the manager's check against the probe", () => {
  describe("what it lets through, the first build still works on", () => {
    test.each(ACCEPTED)("%s", async (_, statements) => {
      // By hand: the first build is not stopped by it.
      expect(await probeOlderBuild(await byHand(statements))).toEqual({ ok: true });
      // Through the manager, on a file with no rows and on one with rows.
      for (const file of [emptyLegacyFile, legacyFile]) {
        const db = await file();
        await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]);
        const recorded = await db.all<{ module: string; version: number }>(
          "SELECT module, version FROM module_schema ORDER BY module",
        );
        expect(recorded).toEqual([
          { module: "host", version: 1 },
          { module: "mileage", version: 2 },
        ]);
        expect(await layout(db)).toBe(1);
        expect(await probeOlderBuild(db)).toEqual({ ok: true });
      }
    });
  });

  describe("what it refuses because the first build breaks on it", () => {
    test.each(BREAKS)("%s", async (_, statements, message, step) => {
      // By hand: the probe fails at this step. This is the control.
      const broken = await probeOlderBuild(await byHand(statements));
      expect(broken).toMatchObject({ ok: false });
      expect((broken as { step: string }).step).toMatch(step);

      // Through the manager: refused, taken back, and the first build is as well off as before.
      const db = await emptyLegacyFile();
      const before = await objectsOf(db);
      const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
        (e: unknown) => e,
      );
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(message);
      expect(await objectsOf(db)).toEqual(before);
      expect(await layout(db)).toBe(1);
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    });
  });

  describe("what it refuses although the probe cannot show the first build break on it", () => {
    test.each(LINT_ONLY)("%s", async (_, statements, message, why) => {
      expect(why.length).toBeGreaterThan(20);
      // The probe passes on it made by hand: this is why it is in this group.
      expect(await probeOlderBuild(await byHand(statements))).toEqual({ ok: true });

      const db = await emptyLegacyFile();
      const before = await objectsOf(db);
      const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
        (e: unknown) => e,
      );
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(message);
      expect(await objectsOf(db)).toEqual(before);
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    });
  });

  test("a legacy table that a migration makes has to be the first layout's, in a new file", async () => {
    // A module that makes the trips differently in a new file: the first build opens that table.
    const changed: SchemaModule = {
      ...MILEAGE_SCHEMA,
      async migrate(tx) {
        await tx.exec(
          [ddl("trip").replace("DEFAULT 'device'", "DEFAULT 'server'"), ddl("point"), ddl("batch")]
            .map((statement) => `${statement};`)
            .join("\n"),
        );
      },
    };
    const db = memoryDb();
    const error = await ensureModules(db, [HOST_SCHEMA, changed]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(
      /made the table trip, which an older build also makes/,
    );
    expect(await objectsOf(db)).toEqual([]);
    expect(await layout(db)).toBe(0);
  });
});

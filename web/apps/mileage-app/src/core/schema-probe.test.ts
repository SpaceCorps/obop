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

/** A table of the first layout dropped and made again in the very text it had. */
const remade = (name: string): string[] => [`DROP TABLE ${name}`, ddl(name)];

/** The columns of the trips as the first layout had them. */
const TRIP_COLUMNS =
  "client_trip_id, server_id, origin, purpose, started_at, ended_at, create_attempted, stop_requested, expected_points, last_seq, end_done, submit_requested, submit_done, discard_reason, discard_done, server_state, distance_m, integrity, reject_reason, attempts, problem_step, problem_code, problem_message, created_ms";

/** Every row of the first layout, by the columns it had: what has to survive any migration. */
const kept = async (db: SqlDb) => ({
  trip: await db.all(`SELECT ${TRIP_COLUMNS} FROM trip ORDER BY client_trip_id`),
  point: await db.all(
    "SELECT client_trip_id, seq, t, lat, lon, accuracy_m, speed_mps FROM point ORDER BY 1, 2",
  ),
  batch: await db.all("SELECT client_trip_id, first_seq, last_seq, sent FROM batch ORDER BY 1, 2"),
  meta: await db.all("SELECT key, value FROM meta ORDER BY key"),
});

/**
 * The files that a migration of the outbox's module is tried on. A file of the first build is
 * upgraded; a new file gets the same migration as its first one, and has to be judged the same.
 * The number is the layout the file has before.
 */
const NEW_FILE = () => Promise.resolve(memoryDb());
const STARTS: [name: string, start: () => Promise<SqlDb>, layoutBefore: number][] = [
  ["a file of the first build", emptyLegacyFile, 1],
  ["a new file", NEW_FILE, 0],
];

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

  // Each step of the probe, and each thing it asks of a step, is there because a file can be made
  // that fails only there: the step is named, and where a step asks several things, so is the thing
  // (the start of the error). A line that nothing could fail would test nothing. One was found
  // and taken out: the count of the second trip's fixes, which is asked right after they were
  // recorded and read back, with nothing written in between that could change it; it is asked at
  // the end of the flow instead ("delete a trip"), where the deletes before it can.
  test.each<[string, string[], RegExp, RegExp?]>([
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
      /^submit and discard/,
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
    // The things a step asks, one at a time. Each file passes everything before and fails here.
    [
      "a fix of the first trip that goes when the second trip's last fix is recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON point WHEN NEW.client_trip_id = 'probe-b' AND NEW.seq = 3 BEGIN DELETE FROM point WHERE client_trip_id = 'probe-a' AND seq = 3; END",
      ],
      /^count the fixes/,
      /^the count: /,
    ],
    [
      "a fix for a trip that has none, left when the second trip's last fix is recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON point WHEN NEW.client_trip_id = 'probe-b' AND NEW.seq = 3 BEGIN INSERT INTO point (client_trip_id, seq, t, lat, lon) VALUES ('probe-none', 1, 0, '0', '0'); END",
      ],
      /^count the fixes/,
      /^the count of a trip without: /,
    ],
    [
      "a fix of the first trip that goes when the first trip is stopped",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.client_trip_id = 'probe-a' AND NEW.stop_requested = 1 AND OLD.stop_requested = 0 BEGIN DELETE FROM point WHERE client_trip_id = 'probe-a' AND seq = 3; END",
      ],
      /^cut and send batches/,
      /^the range: /,
    ],
    [
      "a batch that is sent already, with a higher range, left when the first is cut",
      [
        "CREATE TRIGGER x AFTER INSERT ON batch WHEN NEW.client_trip_id = 'probe-a' AND NEW.first_seq = 1 BEGIN INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES ('probe-a', 9, 99, 1); END",
      ],
      /^cut and send batches/,
      /^the cut: /,
    ],
    [
      "a patch that leaves no problem on the trip",
      [
        "CREATE TRIGGER x AFTER UPDATE OF problem_code ON trip WHEN NEW.problem_code IS NOT NULL BEGIN UPDATE trip SET problem_code = NULL WHERE client_trip_id = NEW.client_trip_id; END",
      ],
      /^patch a trip/,
      /^the patched trip: /,
    ],
    [
      "a clearing that leaves the problem on the trip",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.problem_code IS NULL AND OLD.problem_code IS NOT NULL BEGIN UPDATE trip SET problem_code = OLD.problem_code WHERE client_trip_id = NEW.client_trip_id; END",
      ],
      /^patch a trip/,
      /^the problem is cleared: /,
    ],
    [
      "a first trip that is gone from the list when a trip from the server comes in",
      [
        "CREATE TRIGGER x AFTER INSERT ON trip WHEN NEW.origin = 'server' BEGIN DELETE FROM trip WHERE client_trip_id = 'probe-b'; END",
      ],
      /^list the trips/,
      /^the list does not have probe-b/,
    ],
    [
      "a fix of the second trip that goes when the fixes of the first are deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON point WHEN OLD.client_trip_id = 'probe-a' BEGIN DELETE FROM point WHERE client_trip_id = 'probe-b' AND seq = 3; END",
      ],
      /^delete a trip/,
      /^the other trip is not: /,
    ],
    // What the probe reads back after it wrote, one file for each, so that no read is there for
    // show: every file below does what a trigger or a row that was there already can do to the
    // one thing named, and passes everything before it. (Taking the line out of the probe, one
    // statement at a time, is what showed which of these were missing.)
    [
      "a trip of the first trip's id that is there already",
      [
        "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES ('probe-a', 'private', 'x', 0)",
      ],
      /^start a trip/,
      /^no trip yet: /,
    ],
    [
      "an active trip that is another trip once it is kept",
      [
        "CREATE TRIGGER x AFTER INSERT ON meta WHEN NEW.key = 'active_trip' BEGIN UPDATE meta SET value = 'other' WHERE key = 'active_trip'; END",
      ],
      /^start a trip/,
      /^the active trip: /,
    ],
    [
      "a second trip that is not what was recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON trip WHEN NEW.client_trip_id = 'probe-b' BEGIN UPDATE trip SET attempts = 5 WHERE client_trip_id = 'probe-b'; END",
      ],
      /^start a second trip/,
      /^the second trip: /,
    ],
    [
      "a fix of the first trip that is there before any is recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON trip WHEN NEW.client_trip_id = 'probe-a' BEGIN INSERT INTO point (client_trip_id, seq, t, lat, lon) VALUES ('probe-a', 9, 9, '0', '0'); END",
      ],
      /^record fixes/,
      /^no fix yet: /,
    ],
    [
      "a last fix that is not the one recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON point WHEN NEW.client_trip_id = 'probe-a' AND NEW.seq = 3 BEGIN UPDATE point SET lat = '0' WHERE client_trip_id = 'probe-a' AND seq = 3; END",
      ],
      /^record fixes/,
      /^the last fix: /,
    ],
    [
      "a fix in the middle that is not the one recorded",
      [
        "CREATE TRIGGER x AFTER INSERT ON point WHEN NEW.client_trip_id = 'probe-a' AND NEW.seq = 3 BEGIN UPDATE point SET lat = '0' WHERE client_trip_id = 'probe-a' AND seq = 2; END",
      ],
      /^record fixes/,
      /^the fixes: /,
    ],
    [
      "a stopped trip that has another count of attempts",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.client_trip_id = 'probe-a' AND NEW.stop_requested = 1 AND OLD.stop_requested = 0 BEGIN UPDATE trip SET attempts = 7 WHERE client_trip_id = 'probe-a'; END",
      ],
      /^stop a trip/,
      /^the stopped trip: /,
    ],
    [
      "an active trip that comes back when it is deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON meta WHEN OLD.key = 'active_trip' BEGIN INSERT INTO meta (key, value) VALUES ('active_trip', 'other'); END",
      ],
      /^stop a trip/,
      /^no active trip: /,
    ],
    [
      "an open batch that is there when the first trip is stopped",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.client_trip_id = 'probe-a' AND NEW.stop_requested = 1 AND OLD.stop_requested = 0 BEGIN INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES ('probe-a', 1, 1, 0); END",
      ],
      /^cut and send batches/,
      /^no open batch: /,
    ],
    [
      "a batch that is sent already, with a low range, when the first trip is stopped",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.client_trip_id = 'probe-a' AND NEW.stop_requested = 1 AND OLD.stop_requested = 0 BEGIN INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES ('probe-a', 7, 8, 1); END",
      ],
      /^cut and send batches/,
      /^nothing cut yet: /,
    ],
    [
      "a batch that has another range once it is cut",
      [
        "CREATE TRIGGER x AFTER INSERT ON batch WHEN NEW.client_trip_id = 'probe-a' AND NEW.first_seq = 1 BEGIN UPDATE batch SET last_seq = 2 WHERE client_trip_id = 'probe-a' AND first_seq = 1; END",
      ],
      /^cut and send batches/,
      /^the open batch: /,
    ],
    [
      "a fix in a batch that is not the one recorded, once the batch is cut",
      [
        "CREATE TRIGGER x AFTER INSERT ON batch WHEN NEW.client_trip_id = 'probe-a' BEGIN UPDATE point SET lat = '0' WHERE client_trip_id = 'probe-a' AND seq = 2; END",
      ],
      /^cut and send batches/,
      /^the points of the batch: /,
    ],
    [
      "a batch that is open again once it is sent",
      [
        "CREATE TRIGGER x AFTER UPDATE ON batch WHEN NEW.sent = 1 AND OLD.sent = 0 BEGIN UPDATE batch SET sent = 0 WHERE client_trip_id = NEW.client_trip_id AND first_seq = NEW.first_seq; END",
      ],
      /^cut and send batches/,
      /^no open batch after: /,
    ],
    [
      "a submitted trip that has another count of attempts",
      [
        "CREATE TRIGGER x AFTER UPDATE ON trip WHEN NEW.client_trip_id = 'probe-a' AND NEW.submit_requested = 1 AND OLD.submit_requested = 0 BEGIN UPDATE trip SET attempts = 3 WHERE client_trip_id = 'probe-a'; END",
      ],
      /^submit and discard/,
      /^the submitted trip: /,
    ],
    [
      "a discarded trip that has no end",
      [
        "CREATE TRIGGER x AFTER UPDATE OF discard_reason ON trip WHEN NEW.client_trip_id = 'probe-b' BEGIN UPDATE trip SET ended_at = NULL WHERE client_trip_id = 'probe-b'; END",
      ],
      /^submit and discard/,
      /^the discarded trip: /,
    ],
    [
      "a merged trip that has another distance",
      [
        "CREATE TRIGGER x AFTER UPDATE OF server_state ON trip WHEN NEW.server_state = 'confirmed' BEGIN UPDATE trip SET distance_m = '1' WHERE client_trip_id = NEW.client_trip_id; END",
      ],
      /^merge what the server says/,
      /^the merged trip: /,
    ],
    [
      "a trip from the server that has another count of attempts",
      [
        "CREATE TRIGGER x AFTER INSERT ON trip WHEN NEW.client_trip_id = 'probe-c' BEGIN UPDATE trip SET attempts = 4 WHERE client_trip_id = 'probe-c'; END",
      ],
      /^merge what the server says/,
      /^the trip from the server: /,
    ],
    [
      "a note that is another note once it is updated",
      [
        "CREATE TRIGGER x AFTER UPDATE ON meta WHEN NEW.key = 'probe-note' AND NEW.value = 'two' BEGIN UPDATE meta SET value = 'three' WHERE key = 'probe-note'; END",
      ],
      /^keep a note/,
      /^the note: /,
    ],
    [
      "a note that comes back when it is deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON meta WHEN OLD.key = 'probe-note' BEGIN INSERT INTO meta (key, value) VALUES ('probe-note', 'back'); END",
      ],
      /^keep a note/,
      /^the note is gone: /,
    ],
    [
      "a first trip that comes back when it is deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON trip WHEN OLD.client_trip_id = 'probe-a' BEGIN INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES ('probe-a', 'private', 'x', 0); END",
      ],
      /^delete a trip/,
      /^the trip is gone: /,
    ],
    [
      "a fix of the first trip that comes back when its fixes are deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON point WHEN OLD.client_trip_id = 'probe-a' BEGIN INSERT OR IGNORE INTO point (client_trip_id, seq, t, lat, lon) VALUES ('probe-a', 99, 0, '0', '0'); END",
      ],
      /^delete a trip/,
      /^its fixes are gone: /,
    ],
    [
      "a trip that is made when the third trip is deleted",
      [
        "CREATE TRIGGER x AFTER DELETE ON trip WHEN OLD.client_trip_id = 'probe-c' BEGIN INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES ('probe-z', 'private', 'x', 0); END",
      ],
      /^clear everything/,
      /^no trip: /,
    ],
    [
      "a fix that is made when the last fix of the second trip is cleared",
      [
        "CREATE TRIGGER x AFTER DELETE ON point WHEN OLD.client_trip_id = 'probe-b' AND OLD.seq = 3 BEGIN INSERT OR IGNORE INTO point (client_trip_id, seq, t, lat, lon) VALUES ('probe-b', 99, 0, '0', '0'); END",
      ],
      /^clear everything/,
      /^no fix: /,
    ],
    [
      "a batch that is made when the batch of the second trip is cleared",
      [
        "CREATE TRIGGER x AFTER DELETE ON batch WHEN OLD.client_trip_id = 'probe-b' BEGIN INSERT OR IGNORE INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES ('probe-b', 99, 99, 1); END",
      ],
      /^clear everything/,
      /^no batch: /,
    ],
  ])("fails at the right step for %s", async (_, statements, step, thing) => {
    const result = await probeOlderBuild(await byHand(statements));
    expect(result).toMatchObject({ ok: false });
    expect((result as { step: string }).step).toMatch(step);
    if (thing !== undefined) expect((result as { error: string }).error).toMatch(thing);
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
    "NOT NULL columns whose default is the text NULL, or has the word in it",
    [
      "ALTER TABLE trip ADD COLUMN word TEXT NOT NULL DEFAULT 'NULL'",
      "ALTER TABLE point ADD COLUMN phrase TEXT NOT NULL DEFAULT 'a NULL b'",
      "ALTER TABLE batch ADD COLUMN lower TEXT NOT NULL DEFAULT 'null'",
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
    "the trips made again with a column added where ADD COLUMN puts it, their rows copied back",
    [
      "CREATE TABLE mileage_hold AS SELECT * FROM trip",
      ...rebuilt("trip", /\n\)$/, "\n, extra TEXT)"),
      `INSERT INTO trip (${TRIP_COLUMNS}) SELECT ${TRIP_COLUMNS} FROM mileage_hold`,
      "DROP TABLE mileage_hold",
    ],
  ],
  [
    "the fixes made again in the very text they had, their rows copied back",
    [
      "CREATE TABLE mileage_hold AS SELECT * FROM point",
      ...remade("point"),
      "INSERT INTO point SELECT * FROM mileage_hold",
      "DROP TABLE mileage_hold",
    ],
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
    /^submit and discard/,
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
  // SQLite keeps the last of two defaults, so each of these is a NOT NULL column whose default is
  // NULL, which the first build's insert leaves out. SQLite refuses to add such a column to a
  // table that has rows; on one without rows it adds it.
  [
    "a NOT NULL column with a default, and then a default of NULL",
    ["ALTER TABLE trip ADD COLUMN extra TEXT NOT NULL DEFAULT 'a' DEFAULT NULL"],
    /added the column trip\.extra as NOT NULL without a default/,
    /^start a trip/,
  ],
  [
    "a default of NULL, and then NOT NULL",
    ["ALTER TABLE trip ADD COLUMN extra TEXT DEFAULT NULL NOT NULL"],
    /added the column trip\.extra as NOT NULL without a default/,
    /^start a trip/,
  ],
  [
    "a NOT NULL column with no default",
    ["ALTER TABLE trip ADD COLUMN extra TEXT NOT NULL"],
    /added the column trip\.extra as NOT NULL without a default/,
    /^start a trip/,
  ],
  [
    "a NOT NULL column with a default, and then a default of null in small letters, on the fixes",
    ["ALTER TABLE point ADD COLUMN extra TEXT NOT NULL DEFAULT 'a' DEFAULT null"],
    /added the column point\.extra as NOT NULL without a default/,
    /^record fixes/,
  ],
  [
    "a NOT NULL column with a default, and then a default of NULL, on the batches",
    ["ALTER TABLE batch ADD COLUMN extra TEXT NOT NULL DEFAULT 'a' DEFAULT NULL"],
    /added the column batch\.extra as NOT NULL without a default/,
    /^cut and send batches/,
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
    "the fixes made again with the two columns of the key the other way round",
    rebuilt("point", "PRIMARY KEY (client_trip_id, seq)", "PRIMARY KEY (seq, client_trip_id)"),
    /changed the column point\.client_trip_id/,
    "every statement of the first build names the order it wants; only the order the file keeps the fixes in, and so what a lookup by trip costs, differs",
  ],
  [
    "the trips made again with a column added, written in a tidier way than ADD COLUMN writes it",
    rebuilt("trip", /\n\)$/, ",\n  extra TEXT\n)"),
    /rebuilt the table trip/,
    "the same table; the text is compared letter for letter, so that a piece of it cannot hide",
  ],
  [
    "a second DEFAULT on a column that may be empty",
    ["ALTER TABLE trip ADD COLUMN extra TEXT DEFAULT 'a' DEFAULT 'b'"],
    /added the column trip\.extra with more than one DEFAULT/,
    "SQLite keeps the last one and the first build never sets the column; refused so that the reader never has to say which of two defaults is the one",
  ],
];

describe("the manager's check against the probe", () => {
  describe("what it lets through, the first build still works on", () => {
    test.each(ACCEPTED)("%s", async (_, statements) => {
      // By hand: the first build is not stopped by it.
      expect(await probeOlderBuild(await byHand(statements))).toEqual({ ok: true });
      // Through the manager, in a file with no rows, in one with rows, and in a new file.
      for (const file of [emptyLegacyFile, legacyFile, NEW_FILE]) {
        const db = await file();
        // A new file has no tables yet, and no rows.
        const before =
          file === NEW_FILE ? { trip: [], point: [], batch: [], meta: [] } : await kept(db);
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
        // Every row that was there is there, by every column it had.
        expect(await kept(db)).toEqual(before);
      }
      // The file with rows had rows: the line above compared something.
      const rows = await kept(await legacyFile());
      expect([rows.trip.length, rows.point.length, rows.batch.length, rows.meta.length]).toEqual([
        2, 3, 1, 4,
      ]);
    });
  });

  describe("what it refuses because the first build breaks on it", () => {
    describe.each(STARTS)("in %s", (_start, start, layoutBefore) => {
      test.each(BREAKS)("%s", async (_, statements, message, step) => {
        // By hand: the probe fails at this step. This is the control.
        const broken = await probeOlderBuild(await byHand(statements));
        expect(broken).toMatchObject({ ok: false });
        expect((broken as { step: string }).step).toMatch(step);

        // Through the manager: refused, taken back, and the first build is as well off as before.
        const db = await start();
        const before = await objectsOf(db);
        const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
          (e: unknown) => e,
        );
        expect(error, String(error)).toBeInstanceOf(SchemaError);
        expect((error as Error).message).toMatch(message);
        expect(await objectsOf(db)).toEqual(before);
        expect(await layout(db)).toBe(layoutBefore);
        expect(await probeOlderBuild(db)).toEqual({ ok: true });
      });
    });
  });

  describe("what it refuses although the probe cannot show the first build break on it", () => {
    describe.each(STARTS)("in %s", (_start, start, layoutBefore) => {
      test.each(LINT_ONLY)("%s", async (_, statements, message, why) => {
        expect(why.length).toBeGreaterThan(20);
        // The probe passes on it made by hand: this is why it is in this group.
        expect(await probeOlderBuild(await byHand(statements))).toEqual({ ok: true });

        const db = await start();
        const before = await objectsOf(db);
        const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
          (e: unknown) => e,
        );
        expect(error, String(error)).toBeInstanceOf(SchemaError);
        expect((error as Error).message).toMatch(message);
        expect(await objectsOf(db)).toEqual(before);
        expect(await layout(db)).toBe(layoutBefore);
        expect(await probeOlderBuild(db)).toEqual({ ok: true });
      });
    });
  });

  // A migration that loses the person's rows is one the first build cannot see: its statements
  // work on an empty table as they do on a full one. The manager counts the rows of every table
  // before and after, and a table that has fewer afterwards is refused. Only a file with rows can
  // show it, so these run on the file of the first build with the rows of `legacyFile`.
  describe("what it refuses because rows are lost, which the probe cannot see", () => {
    const LOSS: [name: string, statements: string[], message: RegExp][] = [
      [
        "the fixes dropped and made again in the very text they had",
        remade("point"),
        /lost rows of the table point \(3 before, 0 after\)/,
      ],
      [
        "the trips dropped and made again in the very text they had",
        remade("trip"),
        /lost rows of the table trip \(2 before, 0 after\)/,
      ],
      [
        "the batches dropped and made again in the very text they had",
        remade("batch"),
        /lost rows of the table batch \(1 before, 0 after\)/,
      ],
      [
        "the fixes dropped and made again with a column added, nothing copied back",
        rebuilt("point", /,\n {2}PRIMARY KEY/, ",\n  extra TEXT,\n  PRIMARY KEY"),
        /lost rows of the table point \(3 before, 0 after\)/,
      ],
      [
        "the fixes made again with only some of the rows copied back",
        [
          "CREATE TABLE mileage_hold AS SELECT * FROM point",
          ...remade("point"),
          "INSERT INTO point SELECT * FROM mileage_hold WHERE seq < 3",
          "DROP TABLE mileage_hold",
        ],
        /lost rows of the table point \(3 before, 2 after\)/,
      ],
      [
        "a row of the trips deleted",
        ["DELETE FROM trip WHERE client_trip_id = 'c1'"],
        /lost rows of the table trip \(2 before, 1 after\)/,
      ],
      ["the fixes emptied", ["DELETE FROM point"], /lost rows of the table point \(3 before, 0/],
      [
        "the notes of the host emptied (they are not the module's)",
        ["DELETE FROM meta"],
        /lost rows of the table meta \(4 before, 0 after\)/,
      ],
    ];

    test.each(LOSS)("%s", async (_, statements, message) => {
      // By hand: the first build works on what is left. That is why it is not in the other groups.
      const hand = await legacyFile();
      for (const statement of statements) await hand.exec(statement);
      expect(await probeOlderBuild(hand)).toEqual({ ok: true });

      const db = await legacyFile();
      const before = { objects: await objectsOf(db), rows: await rowsOf(db) };
      const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
        (e: unknown) => e,
      );
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(message);
      // Taken back: the rows are all there, and the file is as it was.
      expect(await objectsOf(db)).toEqual(before.objects);
      expect(await rowsOf(db)).toEqual(before.rows);
      expect(await layout(db)).toBe(1);
    });

    test("a migration may add rows, and a table without rows may be made again", async () => {
      const db = await legacyFile();
      const added = [
        "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES ('c3', 'private', '2026-10-16T09:00:00.000Z', 3)",
        "DELETE FROM batch WHERE 0",
      ];
      await ensureModules(db, [HOST_SCHEMA, mileageAt2(added)]);
      expect((await rowsOf(db)).trip).toHaveLength(3);

      const empty = await emptyLegacyFile();
      await ensureModules(empty, [HOST_SCHEMA, mileageAt2(remade("point"))]);
      expect(await probeOlderBuild(empty)).toEqual({ ok: true });
    });

    test("the count is not a proof: a row replaced by another passes, and the notes say so", async () => {
      const db = await legacyFile();
      await ensureModules(db, [
        HOST_SCHEMA,
        mileageAt2([
          "DELETE FROM batch",
          "INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES ('c9', 1, 1, 0)",
        ]),
      ]);
      expect((await rowsOf(db)).batch).toEqual([
        { client_trip_id: "c9", first_seq: 1, last_seq: 1, sent: 0 },
      ]);
    });
  });

  describe("the first layout in a new file", () => {
    // The tables of the first layout are made by the migration of a new file, and nothing was there
    // to compare them with. They are compared with the first layout that the module declares, so a
    // change that is accepted when it is made to a file of the first build is accepted here, and a
    // change that is refused there is refused here (every shape above runs on both).
    test("a column on the trips is accepted in a new file as in an old one", async () => {
      const statements = ["ALTER TABLE trip ADD COLUMN note TEXT"];
      const fresh = memoryDb();
      await ensureModules(fresh, [HOST_SCHEMA, mileageAt2(statements)]);
      const old = await emptyLegacyFile();
      await ensureModules(old, [HOST_SCHEMA, mileageAt2(statements)]);
      expect(await objectsOf(fresh)).toEqual(await objectsOf(old));
      expect(await probeOlderBuild(fresh)).toEqual({ ok: true });
    });

    test.each<[string, string[]]>([
      ["the trips are not made", ["DROP TABLE trip"]],
      ["the trips are renamed", ["ALTER TABLE trip RENAME TO mileage_trip"]],
      ["the batches are dropped", ["DROP TABLE batch"]],
    ])("a new file in which %s is refused, as an old one is", async (_, statements) => {
      for (const [, start, layoutBefore] of STARTS) {
        const db = await start();
        const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
          (e: unknown) => e,
        );
        expect(error, String(error)).toBeInstanceOf(SchemaError);
        expect((error as Error).message).toMatch(/^module mileage removed the table (trip|batch)/);
        expect(await layout(db)).toBe(layoutBefore);
      }
    });

    test("a table of its own with a foreign key to the trips is refused in a new file, as in an old one", async () => {
      const statements = [
        "CREATE TABLE mileage_note (id TEXT PRIMARY KEY NOT NULL, trip TEXT REFERENCES trip(client_trip_id))",
      ];
      for (const [, start] of STARTS) {
        const error = await ensureModules(await start(), [
          HOST_SCHEMA,
          mileageAt2(statements),
        ]).catch((e: unknown) => e);
        expect(error, String(error)).toBeInstanceOf(SchemaError);
        expect((error as Error).message).toMatch(
          /made the table mileage_note with a foreign key to trip/,
        );
      }
    });

    test("a module that makes the trips differently in a new file is refused: the first build opens that table", async () => {
      const changed: SchemaModule = {
        ...MILEAGE_SCHEMA,
        async migrate(tx) {
          await tx.exec(
            [
              ddl("trip").replace("DEFAULT 'device'", "DEFAULT 'server'"),
              ddl("point"),
              ddl("batch"),
            ]
              .map((statement) => `${statement};`)
              .join("\n"),
          );
        },
      };
      const db = memoryDb();
      const error = await ensureModules(db, [HOST_SCHEMA, changed]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(/changed the column trip\.origin/);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });

    test("the same module with the other change that the first build cannot live with, in a new file", async () => {
      // The trips made with a CHECK that nothing satisfies, in the migration of a new file.
      const changed: SchemaModule = {
        ...MILEAGE_SCHEMA,
        async migrate(tx) {
          await tx.exec(
            [ddl("trip").replace(/\n\)$/, ",\n  CHECK (0)\n)"), ddl("point"), ddl("batch")]
              .map((statement) => `${statement};`)
              .join("\n"),
          );
        },
      };
      const db = memoryDb();
      const error = await ensureModules(db, [HOST_SCHEMA, changed]).catch((e: unknown) => e);
      expect((error as Error).message).toMatch(/rebuilt the table trip/);
      // And the file the first build would have made from it does break the first build.
      const hand = await emptyLegacyFile();
      for (const statement of rebuilt("trip", /\n\)$/, ",\n  CHECK (0)\n)")) {
        await hand.exec(statement);
      }
      expect(await probeOlderBuild(hand)).toMatchObject({ ok: false, step: "start a trip" });
    });
  });
});

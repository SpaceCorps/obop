import { DatabaseSync } from "node:sqlite";
import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "./host-state.ts";
import {
  SchemaError,
  checkModules,
  ensureModules,
  forbiddenInMigration,
  transactionControl,
  type SchemaModule,
} from "./schema.ts";
import type { SqlDb, SqlTx } from "./sql.ts";
import { MILEAGE_SCHEMA } from "./sql-store.ts";
import { emptyLegacyFile, legacyFile, seed } from "../test-support/legacy-file.ts";
import { migrate as firstBuildMigration } from "../test-support/legacy-schema.ts";
import { memoryDb, sqlDbOver } from "../test-support/memory-db.ts";
import { probeOlderBuild } from "../test-support/older-build-probe.ts";
import { tempDatabase } from "../test-support/temp-db.ts";

// A migration is given the transaction the manager holds, and the connection under it is the one
// the app goes on using. This file holds what the guard refuses besides a statement that controls
// the transaction (the words ROLLBACK, TEMP, TEMPORARY, ATTACH and DETACH at any place, the name temp
// as a schema qualifier, any PRAGMA but user_version, and the time of the insert as a default in an
// ALTER TABLE), the statements of the manager that name their schema, and the look at the
// temporary schema afterwards. `schema-transaction.test.ts` holds the six words that begin, end or
// nest a transaction, the savepoint check and the versions.

/** The word in lower case, in capitals, with one capital, and with the letters alternating. */
const casings = (word: string): string[] => [
  word.toLowerCase(),
  word.toUpperCase(),
  word[0]!.toUpperCase() + word.slice(1).toLowerCase(),
  word
    .split("")
    .map((c, i) => (i % 2 === 0 ? c.toLowerCase() : c.toUpperCase()))
    .join(""),
];

/** In front of a text: nothing, a comment of either kind, another statement, and both. */
const PREFIXES = ["", "/* a */ ", "-- a\n", "SELECT 1; ", "SELECT 1; /* c */ -- d\n "];

const ran = (sql: string) => forbiddenInMigration(sql)?.ran ?? null;

// The reasons, written out here as the messages say them, so that a change of a message fails.
const WHY = {
  rollback:
    "a ROLLBACK conflict clause (INSERT OR ROLLBACK, ON CONFLICT ROLLBACK, RAISE(ROLLBACK, ...)) ends the transaction that the manager holds as the statement does, so a migration may not use the word at all (a column called rollback is written as a quoted name)",
  temp: "objects in the temporary schema last as long as the connection and take the place of the tables of the file for any name that is not qualified, so a migration may not make one (a column called temp is written as a quoted name)",
  attach:
    "a database that is attached to the connection stays attached when the migration ends, also when it is refused and taken back, so a migration may not attach or detach one",
  pragma:
    "a pragma changes the connection (its locking mode, what it may write, how it treats foreign keys), the app keeps using that connection, and a migration that is refused does not take it back; the one pragma a migration may use is user_version, which the lint compares before and after",
  alter:
    "SQLite refuses a default of that kind on a column added to a table that has rows and accepts it on a table with none, so a migration with one passes on a new file and fails on a phone with trips, also where a later statement makes the table again; the text of an ALTER TABLE may not have the word",
  optimize:
    "the table-valued function pragma_optimize runs PRAGMA optimize, which can run ANALYZE and write the table sqlite_stat1 into the file, and the check afterwards does not look at the tables of SQLite's own, so a migration may not name it",
};

describe("the reader of the words that reach past the transaction", () => {
  test.each([
    ["INSERT OR ROLLBACK", "INSERT OR ROLLBACK INTO a_t (x) VALUES (1)"],
    ["UPDATE OR ROLLBACK", "UPDATE OR ROLLBACK a_t SET x = 1"],
    ["a column constraint", "CREATE TABLE a_t (x TEXT NOT NULL ON CONFLICT ROLLBACK)"],
    ["a key", "CREATE TABLE a_t (x TEXT PRIMARY KEY ON CONFLICT ROLLBACK)"],
    ["a table constraint", "CREATE TABLE a_t (x TEXT, UNIQUE (x) ON CONFLICT ROLLBACK)"],
    ["RAISE", "SELECT RAISE(ROLLBACK, 'no')"],
    [
      "a trigger body",
      "CREATE TRIGGER a_g BEFORE INSERT ON a_t BEGIN SELECT RAISE(ROLLBACK, 'no'); END",
    ],
    [
      "the second statement of a trigger body",
      "CREATE TRIGGER a_g BEFORE INSERT ON a_t BEGIN SELECT 1; SELECT RAISE(ROLLBACK, 'no'); END",
    ],
    [
      "a statement after others",
      "INSERT INTO a_t (x) VALUES (1) ON CONFLICT DO NOTHING; INSERT OR ROLLBACK INTO a_t (x) VALUES (1)",
    ],
  ])("finds ROLLBACK in %s, in every casing and behind every prefix", (_, form) => {
    for (const casing of casings("ROLLBACK")) {
      for (const prefix of PREFIXES) {
        const sql = prefix + form.replace("ROLLBACK", casing);
        expect(ran(sql), sql).toBe("text with the word ROLLBACK");
        expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.rollback);
      }
    }
  });

  test.each([
    ["TEMP", "CREATE TEMP TABLE a_t (x TEXT)"],
    ["TEMPORARY", "CREATE TEMPORARY TABLE a_t (x TEXT)"],
    ["TEMP", "CREATE TEMP VIEW a_v AS SELECT 1"],
    ["TEMP", "CREATE TEMP TRIGGER a_g BEFORE INSERT ON a_t BEGIN SELECT 1; END"],
    ["TEMPORARY", "CREATE TEMPORARY TRIGGER a_g BEFORE INSERT ON a_t BEGIN SELECT 1; END"],
    ["TEMP", "CREATE TEMP INDEX a_i ON a_t (x)"],
    ["TEMP", "INSERT INTO TEMP.a_t (x) VALUES (1)"],
    ["TEMP", "CREATE TABLE a_t (TEMP TEXT)"],
  ])("finds the word %s in %j, in every casing and behind every prefix", (word, form) => {
    for (const casing of casings(word)) {
      for (const prefix of PREFIXES) {
        const sql = prefix + form.replace(word, casing);
        expect(ran(sql), sql).toBe(`text with the word ${word}`);
        expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.temp);
      }
    }
  });

  // The name of the schema can be written as a quoted name or as a string, and SQLite takes all of
  // them (reproduced on node:sqlite: each of the first three makes a table in the temporary schema).
  test.each([
    '"temp".a_t',
    "[temp].a_t",
    "`temp`.a_t",
    "'temp'.a_t",
    '"TEMP".a_t',
    '"Temp"  .  a_t',
    '"temp" /* c */ . a_t',
    '"temp"\n.a_t',
  ])("finds the schema name temp as %j", (qualifier) => {
    for (const sql of [
      `CREATE TABLE ${qualifier} (x TEXT)`,
      `INSERT INTO ${qualifier} (x) VALUES (1)`,
      `SELECT * FROM ${qualifier}`,
      `DROP TABLE ${qualifier}`,
      `/* a */ CREATE TABLE ${qualifier} (x TEXT)`,
      `SELECT 1; CREATE TABLE ${qualifier} (x TEXT)`,
    ]) {
      expect(ran(sql), sql).toBe("text with the schema name temp");
      expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.temp);
    }
  });

  // The first two forms of each word begin with it; the other three do not (behind EXPLAIN, as a bare
  // column name in a CREATE TABLE and in a SELECT), so the word is refused at those places too.
  test.each([
    ["ATTACH", "ATTACH DATABASE ':memory:' AS z"],
    ["ATTACH", "ATTACH ':memory:' AS z"],
    ["ATTACH", "EXPLAIN ATTACH ':memory:' AS z"],
    ["ATTACH", "CREATE TABLE a_t (ATTACH TEXT)"],
    ["ATTACH", "SELECT ATTACH FROM a_t"],
    ["DETACH", "DETACH DATABASE z"],
    ["DETACH", "DETACH z"],
    ["DETACH", "EXPLAIN DETACH z"],
    ["DETACH", "CREATE TABLE a_t (DETACH TEXT)"],
    ["DETACH", "SELECT DETACH FROM a_t"],
  ])("finds the word %s in %j, in every casing and behind every prefix", (word, form) => {
    for (const casing of casings(word)) {
      for (const prefix of PREFIXES) {
        const sql = prefix + form.replace(word, casing);
        expect(ran(sql), sql).toBe(`text with the word ${word}`);
        expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.attach);
      }
    }
  });

  test.each([
    ["PRAGMA locking_mode = EXCLUSIVE", "PRAGMA locking_mode"],
    ["PRAGMA query_only = 1", "PRAGMA query_only"],
    ["PRAGMA foreign_keys = OFF", "PRAGMA foreign_keys"],
    ["PRAGMA writable_schema = ON", "PRAGMA writable_schema"],
    ["PRAGMA busy_timeout = 1", "PRAGMA busy_timeout"],
    ["PRAGMA recursive_triggers = ON", "PRAGMA recursive_triggers"],
    ["PRAGMA table_info(trip)", "PRAGMA table_info"],
    ["PRAGMA main.user_version", "PRAGMA main"],
    ["PRAGMA main.user_version = 2", "PRAGMA main"],
    ["PRAGMA 'user_version'", "PRAGMA user_version"],
    ["PRAGMA", "PRAGMA"],
    ["EXPLAIN PRAGMA locking_mode = EXCLUSIVE", "PRAGMA locking_mode"],
    ["SELECT 1; PRAGMA locking_mode = EXCLUSIVE", "PRAGMA locking_mode"],
  ])("finds %j", (form, found) => {
    for (const casing of ["pragma", "PRAGMA", "Pragma", "pRaGmA"]) {
      for (const prefix of PREFIXES) {
        const sql = prefix + form.replace(/PRAGMA/i, casing);
        expect(ran(sql), sql).toBe(found);
        expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.pragma);
      }
    }
  });

  test.each([
    "PRAGMA user_version",
    "PRAGMA user_version = 1",
    "PRAGMA user_version = 0",
    "PRAGMA user_version(1)",
    "pragma USER_VERSION = 1",
    "PRAGMA User_Version",
    "SELECT 1; PRAGMA user_version = 1",
    "PRAGMA user_version = 1; SELECT 2",
  ])("lets %j through: it is the one pragma the lint judges", (sql) => {
    expect(forbiddenInMigration(sql), sql).toBeNull();
  });

  // Limit 5 of 8.7.1: a column that is called temp, rollback, attach, detach or pragma, written as a
  // bare word, is refused (the guard fails closed); written as a quoted name it is not. A cost and
  // not a hole, and held here, in four casings, for a CREATE TABLE and for an ALTER.
  test.each([
    ["temp", "text with the word TEMP", WHY.temp],
    ["rollback", "text with the word ROLLBACK", WHY.rollback],
    ["attach", "text with the word ATTACH", WHY.attach],
    ["detach", "text with the word DETACH", WHY.attach],
    ["pragma", "PRAGMA TEXT", WHY.pragma],
  ])(
    "a column called %s is refused as a bare word and not as a quoted name",
    (name, found, why) => {
      for (const casing of casings(name)) {
        const shapes = (column: string) => [
          `CREATE TABLE a_t (${column} TEXT)`,
          `ALTER TABLE a_t ADD COLUMN ${column} TEXT`,
        ];
        for (const sql of shapes(casing)) {
          expect(ran(sql), sql).toBe(found);
          expect(forbiddenInMigration(sql)?.why, sql).toBe(why);
        }
        for (const quoted of [`"${casing}"`, `[${casing}]`, `\`${casing}\``]) {
          for (const sql of shapes(quoted)) expect(forbiddenInMigration(sql), sql).toBeNull();
        }
      }
    },
  );

  test.each(["ALTER", "alter", "Alter"])(
    "finds the time of the insert in a statement that begins with %s",
    (alter) => {
      for (const word of ["CURRENT_TIME", "CURRENT_DATE", "CURRENT_TIMESTAMP"]) {
        for (const casing of [word, word.toLowerCase(), word[0] + word.slice(1).toLowerCase()]) {
          for (const form of [
            `${alter} TABLE t ADD COLUMN c TEXT DEFAULT ${casing}`,
            `${alter} TABLE t ADD COLUMN c TEXT NOT NULL DEFAULT ${casing}`,
            `${alter} TABLE t ADD COLUMN c TEXT DEFAULT (${casing})`,
            `${alter} TABLE t ADD c TEXT DEFAULT ${casing} NOT NULL`,
            `/* a */ ${alter} TABLE t ADD COLUMN c TEXT DEFAULT ${casing}`,
            `-- a\n${alter} TABLE t ADD COLUMN c TEXT DEFAULT ${casing}`,
            `SELECT 1; ${alter} TABLE t ADD COLUMN c TEXT DEFAULT ${casing}`,
          ]) {
            expect(ran(form), form).toBe(`an ALTER TABLE with ${word}`);
            expect(forbiddenInMigration(form)?.why, form).toBe(WHY.alter);
          }
        }
      }
    },
  );

  // What is not refused: a word in a string, a comment or a quoted name; a word that only begins
  // or ends like one; the time of the insert anywhere but in an ALTER statement (a default in a
  // CREATE TABLE is not looked at by the reader); the one pragma.
  test.each([
    "",
    "SELECT 1",
    "SELECT 'ROLLBACK'",
    "SELECT 'INSERT OR ROLLBACK; TEMP; ATTACH; PRAGMA locking_mode; ALTER CURRENT_TIME'",
    "SELECT 1 /* ROLLBACK TEMP TEMPORARY ATTACH DETACH PRAGMA locking_mode */",
    "SELECT 1 -- ROLLBACK TEMP ATTACH PRAGMA locking_mode\n",
    'CREATE TABLE a_t ("rollback" TEXT, "temp" TEXT, "attach" TEXT, "detach" TEXT, "pragma" TEXT)',
    "CREATE TABLE a_t ([rollback] TEXT, [temp] TEXT, `attach` TEXT)",
    "SELECT x'726f6c6c6261636b'",
    "INSERT INTO a_t (x) VALUES ('temp')",
    "INSERT INTO a_t (x) VALUES ('temp'), ('TEMP.x')",
    'SELECT "temp" FROM a_t',
    "SELECT 'temp', [temp], `temp` FROM a_t",
    "SELECT a_t.x FROM a_t WHERE x = 'temp' OR x = 'a.temp'",
    "SELECT temperature, template, tempo, a_temp, temp_x FROM a_t",
    "SELECT attached, detached_x, rollbacks, rollback_x, pragma_table_info FROM a_t",
    "SELECT name FROM pragma_table_info('a_t')",
    "SELECT user_version FROM pragma_user_version",
    "SELECT 1; SELECT name FROM pragma_index_list('a_t')",
    "CREATE TABLE a_t (c TEXT DEFAULT CURRENT_TIMESTAMP)",
    "INSERT INTO a_t (c) VALUES (CURRENT_TIME)",
    "SELECT CURRENT_DATE",
    "UPDATE a_t SET c = CURRENT_TIMESTAMP",
    "ALTER TABLE a_t ADD COLUMN c TEXT DEFAULT 'CURRENT_TIME'",
    'ALTER TABLE a_t ADD COLUMN "current_time" TEXT',
    "ALTER TABLE a_t /* CURRENT_DATE */ ADD COLUMN c TEXT",
    "ALTER TABLE a_t ADD COLUMN current_x TEXT",
    "ALTER TABLE a_t ADD COLUMN c TEXT; SELECT CURRENT_TIME",
    // An ALTER statement is one that begins with the word: EXPLAIN shows what a statement would do
    // and does not do it.
    "EXPLAIN ALTER TABLE a_t ADD COLUMN c TEXT DEFAULT CURRENT_TIME",
    "SELECT 1; SELECT CURRENT_TIME; ALTER TABLE a_t ADD COLUMN c TEXT",
  ])("lets %j through", (sql) => {
    expect(forbiddenInMigration(sql), sql).toBeNull();
  });

  test("answers the first one found in the text, and the statements that were read when the rest is not readable", () => {
    expect(ran("PRAGMA locking_mode = EXCLUSIVE; CREATE TEMP TABLE t (x)")).toBe(
      "PRAGMA locking_mode",
    );
    expect(ran("CREATE TEMP TABLE t (x); PRAGMA locking_mode = EXCLUSIVE")).toBe(
      "text with the word TEMP",
    );
    // The statement before a trigger that does not end was read; the trigger was not.
    expect(
      ran("ATTACH ':memory:' AS z; CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1"),
    ).toBe("text with the word ATTACH");
    expect(ran("CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1")).toBeNull();
  });

  // After a NUL a driver may or may not go on reading, so the reader cannot tell what runs.
  test.each([
    "SELECT 1\0; COMMIT",
    "SELECT 1; COMMIT\0",
    "\0COMMIT",
    "SELECT 1;\0COMMIT",
    "\0",
    "SELECT 'a\0b'",
    "-- a\0\nSELECT 1",
    "/* \0 */ SELECT 1",
    "SELECT 1 \0",
  ])("a NUL in %j makes the text unreadable", (sql) => {
    expect(transactionControl(sql)).toBe("unreadable");
    // It reads nothing of a text it cannot split.
    expect(forbiddenInMigration(sql)).toBeNull();
  });
});

type Method = "exec" | "run" | "one" | "all";
const METHODS: Method[] = ["exec", "run", "one", "all"];
const call = (tx: SqlTx, method: Method, sql: string) =>
  method === "exec"
    ? tx.exec(sql)
    : method === "run"
      ? tx.run(sql, [])
      : method === "one"
        ? tx.one(sql)
        : tx.all(sql);

/** A module that makes a table, runs one text by one method, and then notes that it went on. */
function running(id: string, method: Method, sql: string) {
  const reached: string[] = [];
  const m: SchemaModule & { reached: string[] } = {
    id,
    version: 1,
    reached,
    async migrate(tx) {
      await tx.exec(`CREATE TABLE ${id}_items (id TEXT)`);
      await call(tx, method, sql);
      reached.push("after");
    },
  };
  return m;
}

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

describe("a migration that uses a word that reaches past its transaction", () => {
  // One text for each word, refused before it runs by each method that runs text, with the
  // message, and the table the migration made first is taken back with the rest.
  const TEXTS: [string, string, string, string][] = [
    [
      "ROLLBACK",
      "INSERT OR ROLLBACK INTO a_items (id) VALUES ('x')",
      "text with the word ROLLBACK",
      WHY.rollback,
    ],
    ["TEMP", "CREATE TEMP TABLE a_scratch (id TEXT)", "text with the word TEMP", WHY.temp],
    [
      "the schema name temp",
      'CREATE TABLE "temp".a_scratch (id TEXT)',
      "text with the schema name temp",
      WHY.temp,
    ],
    ["ATTACH", "ATTACH DATABASE ':memory:' AS z", "text with the word ATTACH", WHY.attach],
    ["DETACH", "DETACH DATABASE z", "text with the word DETACH", WHY.attach],
    ["PRAGMA locking_mode", "PRAGMA locking_mode = EXCLUSIVE", "PRAGMA locking_mode", WHY.pragma],
    [
      "CURRENT_TIME in an ALTER TABLE",
      "ALTER TABLE a_items ADD COLUMN stamp TEXT DEFAULT CURRENT_TIME",
      "an ALTER TABLE with CURRENT_TIME",
      WHY.alter,
    ],
  ];

  describe.each(METHODS)("through %s", (method) => {
    test.each(TEXTS)("%s is refused before it runs", async (_, sql, what, why) => {
      const db = memoryDb();
      const m = running("a", method, sql);
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(`module a ran ${what}; ${why}`);
      expect(m.reached).toEqual([]);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });
  });

  test("the module's id is in the message, and an earlier module is taken back with it", async () => {
    const db = memoryDb();
    const first: SchemaModule = {
      id: "first",
      version: 1,
      migrate: (tx) => tx.exec("CREATE TABLE first_items (id TEXT)"),
    };
    const error = await ensureModules(db, [
      first,
      running("second", "exec", "PRAGMA query_only = 1"),
    ]).catch((e: unknown) => e);
    expect((error as Error).message).toBe(`module second ran PRAGMA query_only; ${WHY.pragma}`);
    expect(await objectsOf(db)).toEqual([]);
  });

  // The words in a string, a comment or a quoted name do no harm, and the one pragma is allowed.
  test("a string, a comment, a quoted name and the one pragma are accepted", async () => {
    const db = memoryDb();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec(
          'CREATE TABLE a_words ("rollback" TEXT, "temp" TEXT, [attach] TEXT, `detach` TEXT, "pragma" TEXT)',
        );
        await tx.run("INSERT INTO a_words (\"temp\") VALUES ('TEMP; ROLLBACK; ATTACH x AS y')");
        await tx.exec("/* TEMP ROLLBACK ATTACH */ ALTER TABLE a_words ADD COLUMN note TEXT");
        await tx.exec("ALTER TABLE a_words ADD COLUMN stamp TEXT DEFAULT 'CURRENT_TIME'");
        await tx.exec("CREATE TABLE a_when (c TEXT DEFAULT CURRENT_TIMESTAMP)");
        expect(await tx.one("PRAGMA user_version")).toEqual({ user_version: 0 });
        expect(
          await tx.all("SELECT name FROM pragma_table_info('a_words') ORDER BY cid"),
        ).toHaveLength(7);
        await tx.exec("PRAGMA user_version = 0");
      },
    };
    await ensureModules(db, [m]);
    expect((await objectsOf(db)).map((o) => o.name)).toEqual([
      "a_when",
      "a_words",
      "module_schema",
    ]);
    expect(await layout(db)).toBe(1);
  });

  // A migration that catches the refusal and goes on has run nothing of the refused text.
  test("a refusal that the migration catches leaves nothing behind", async () => {
    const db = memoryDb();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_items (id TEXT PRIMARY KEY)");
        await tx.exec("INSERT INTO a_items (id) VALUES ('x')");
        for (const sql of [
          "INSERT OR ROLLBACK INTO a_items (id) VALUES ('x')",
          "CREATE TEMP TABLE a_scratch (id TEXT)",
          "ATTACH DATABASE ':memory:' AS z",
          "PRAGMA locking_mode = EXCLUSIVE",
        ]) {
          await tx.exec(sql).catch(() => undefined);
        }
        await tx.exec("INSERT INTO a_items (id) VALUES ('y')");
      },
    };
    await ensureModules(db, [m]);
    expect(await db.all("SELECT id FROM a_items ORDER BY id")).toEqual([{ id: "x" }, { id: "y" }]);
    expect(await db.all("SELECT name FROM sqlite_temp_master")).toEqual([]);
    expect(await attachedTo(db)).toEqual([]);
    expect(await db.one("PRAGMA locking_mode")).toEqual({ locking_mode: "normal" });
  });
});

/** The databases other than the file and the temporary schema that the connection has attached. */
const attachedTo = async (db: SqlDb) =>
  (await db.all<{ name: string }>("SELECT name FROM pragma_database_list"))
    .map((d) => d.name)
    .filter((name) => name !== "main" && name !== "temp");

/** A module of the app at version 2, on a file of the first build, doing `work` after its own step. */
const mileageAt2 = (work: (tx: SqlTx) => Promise<void>): SchemaModule => ({
  ...MILEAGE_SCHEMA,
  version: 2,
  async migrate(tx, from) {
    await MILEAGE_SCHEMA.migrate(tx, from);
    if (from < 2) await work(tx);
  },
});

/** The same, over statements run one after the other. */
const statements = (list: readonly string[]) =>
  mileageAt2(async (tx) => {
    for (const statement of list) await tx.exec(statement);
  });

async function expectUntouched(db: SqlDb, run: () => Promise<unknown>) {
  const before = { objects: await objectsOf(db), rows: await rowsOf(db) };
  const error = await run().catch((e: unknown) => e);
  expect(error, String(error)).toBeInstanceOf(SchemaError);
  expect(await objectsOf(db)).toEqual(before.objects);
  expect(await rowsOf(db)).toEqual(before.rows);
  expect(await layout(db)).toBe(1);
  expect(await probeOlderBuild(db)).toEqual({ ok: true });
  return (error as Error).message;
}

// Four files in which a rollback comes from a conflict clause and not from a statement. The
// migration catches the refusal of the guard and goes on, as it did when nothing refused: the
// statement that follows is the one that did the harm, and it is taken back with the rest.
describe("a rollback by conflict clause, on a file of the first build with rows", () => {
  test("INSERT OR ROLLBACK, caught, and then DROP TABLE trip", async () => {
    const db = await legacyFile();
    const message = await expectUntouched(db, () =>
      ensureModules(db, [
        HOST_SCHEMA,
        mileageAt2(async (tx) => {
          await tx
            .exec("INSERT OR ROLLBACK INTO meta (key, value) VALUES ('owner', 'x')")
            .catch(() => undefined);
          await tx.exec("DROP TABLE trip");
        }),
      ]),
    );
    expect(message).toMatch(
      /^module mileage removed the table trip, which an older build still uses/,
    );
  });

  test("a column constraint ON CONFLICT ROLLBACK, caught, and then DELETE FROM point", async () => {
    const db = await legacyFile();
    const message = await expectUntouched(db, () =>
      ensureModules(db, [
        HOST_SCHEMA,
        mileageAt2(async (tx) => {
          await tx
            .exec("CREATE TABLE mileage_y (a TEXT NOT NULL ON CONFLICT ROLLBACK)")
            .catch(() => undefined);
          await tx.exec("INSERT INTO mileage_y VALUES (NULL)").catch(() => undefined);
          await tx.exec("DELETE FROM point");
        }),
      ]),
    );
    expect(message).toMatch(/^module mileage lost rows of the table point \(3 before, 0 after\)/);
  });

  test("RAISE(ROLLBACK) in a trigger, caught, and then DELETE FROM point", async () => {
    const db = await legacyFile();
    const message = await expectUntouched(db, () =>
      ensureModules(db, [
        HOST_SCHEMA,
        mileageAt2(async (tx) => {
          await tx.exec("CREATE TABLE mileage_z (a TEXT)");
          await tx
            .exec(
              "CREATE TRIGGER mileage_t BEFORE INSERT ON mileage_z BEGIN SELECT RAISE(ROLLBACK, 'no'); END",
            )
            .catch(() => undefined);
          await tx.exec("INSERT INTO mileage_z VALUES ('x')").catch(() => undefined);
          await tx.exec("DELETE FROM point");
        }),
      ]),
    );
    expect(message).toMatch(/^module mileage lost rows of the table point \(3 before, 0 after\)/);
  });

  test("a duplicate INSERT OR ROLLBACK, caught, and then a new table: the migration is whole and is accepted", async () => {
    const db = await legacyFile();
    await ensureModules(db, [
      HOST_SCHEMA,
      mileageAt2(async (tx) => {
        await tx.exec("CREATE TABLE mileage_items (id TEXT PRIMARY KEY)");
        await tx.exec("INSERT INTO mileage_items VALUES ('a')");
        await tx
          .exec("INSERT OR ROLLBACK INTO mileage_items VALUES ('a'), ('a')")
          .catch(() => undefined);
        await tx.exec("CREATE TABLE mileage_more (id TEXT)");
      }),
    ]);
    expect((await objectsOf(db)).map((o) => o.name)).toEqual(
      expect.arrayContaining(["mileage_items", "mileage_more", "module_schema"]),
    );
    expect(await db.all("SELECT * FROM module_schema ORDER BY module")).toEqual([
      { module: "host", version: 1 },
      { module: "mileage", version: 2 },
    ]);
    expect(await db.all("SELECT id FROM mileage_items")).toEqual([{ id: "a" }]);
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
  });
});

describe("the temporary schema", () => {
  test("the repro: a temporary module_schema, copied from the real one, is refused before it is made", async () => {
    const db = await legacyFile();
    const message = await expectUntouched(db, () =>
      ensureModules(db, [
        HOST_SCHEMA,
        statements([
          "CREATE TEMP TABLE module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) WITHOUT ROWID",
          "INSERT INTO temp.module_schema SELECT * FROM main.module_schema",
          "ALTER TABLE trip ADD COLUMN note TEXT",
        ]),
      ]),
    );
    expect(message).toBe(`module mileage ran text with the word TEMP; ${WHY.temp}`);
  });

  test("a temporary trigger that sets a version, or that stops the writes, is refused", async () => {
    for (const trigger of [
      "CREATE TEMP TRIGGER zpoison AFTER INSERT ON main.module_schema BEGIN UPDATE main.module_schema SET version = 99 WHERE module = NEW.module; END",
      "CREATE TEMP TRIGGER zroll BEFORE INSERT ON main.meta BEGIN SELECT RAISE(ABORT, 'no'); END",
    ]) {
      const db = memoryDb();
      const error = await ensureModules(db, [HOST_SCHEMA, statements([trigger])]).catch(
        (e: unknown) => e,
      );
      expect((error as Error).message, trigger).toBe(
        `module mileage ran text with the word TEMP; ${WHY.temp}`,
      );
      expect(await objectsOf(db)).toEqual([]);
    }
  });

  // The words are the first line of defence; this is the look at what they miss. A migration that
  // has a way to the connection other than the transaction it was given (the closure over the
  // driver that nothing in the app has) is the only way to a temporary object past the words.
  describe("what a migration does to the temporary schema by another way is found afterwards", () => {
    const CHANGED =
      "changed the temporary schema; objects in it last as long as the connection and take the place of the tables of the file for any name that is not qualified";
    function viaTheConnection(setup: string | null, work: (sqlite: DatabaseSync) => void) {
      const sqlite = new DatabaseSync(":memory:");
      const db = sqlDbOver(sqlite);
      if (setup !== null) sqlite.exec(setup);
      const m: SchemaModule = {
        id: "a",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE a_items (id TEXT)");
          work(sqlite);
        },
      };
      return { db, m };
    }

    test.each([
      ["a table", "CREATE TEMP TABLE zz (a TEXT)"],
      ["a view", "CREATE TEMP VIEW zz AS SELECT 1 AS a"],
      [
        "a trigger",
        "CREATE TEMP TABLE zz (a TEXT); CREATE TEMP TRIGGER zt AFTER INSERT ON zz BEGIN SELECT 1; END",
      ],
      ["a table that shadows one of the file", "CREATE TEMP TABLE a_items (a TEXT)"],
    ])("%s made by the connection is refused and taken back", async (_, sql) => {
      const { db, m } = viaTheConnection(null, (sqlite) => sqlite.exec(sql));
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(`module a ${CHANGED}`);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });

    test("a temporary table that was there is refused when it is dropped, and when it is made again with another text", async () => {
      for (const work of [
        "DROP TABLE zz",
        "DROP TABLE zz; CREATE TEMP TABLE zz (a TEXT, b TEXT)",
      ]) {
        const { db, m } = viaTheConnection("CREATE TEMP TABLE zz (a TEXT)", (sqlite) =>
          sqlite.exec(work),
        );
        const error = await ensureModules(db, [m]).catch((e: unknown) => e);
        expect((error as Error).message, work).toBe(`module a ${CHANGED}`);
      }
    });

    // The temporary schema is compared by what it holds, not by how much: a table made again with
    // another text of the same length, or a table renamed to a name of the same length, is a change.
    test.each([
      [
        "made again with another column of the same length",
        "DROP TABLE zz; CREATE TEMP TABLE zz (b TEXT)",
      ],
      ["renamed to a name of the same length", "ALTER TABLE zz RENAME TO yy"],
      [
        "made again with another type of the same length",
        "DROP TABLE zz; CREATE TEMP TABLE zz (a REAL)",
      ],
    ])("a temporary table that was there and is %s is refused", async (_, work) => {
      const { db, m } = viaTheConnection("CREATE TEMP TABLE zz (a TEXT)", (sqlite) =>
        sqlite.exec(work),
      );
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(`module a ${CHANGED}`);
      expect(await objectsOf(db)).toEqual([]);
    });

    test("a temporary schema that has the same objects afterwards is not a finding, in whatever order they were made", async () => {
      const { db, m } = viaTheConnection(
        "CREATE TEMP TABLE zx (a TEXT); CREATE TEMP TABLE zy (a TEXT)",
        (sqlite) =>
          sqlite.exec(
            "DROP TABLE zx; DROP TABLE zy; CREATE TEMP TABLE zy (a TEXT); CREATE TEMP TABLE zx (a TEXT)",
          ),
      );
      await ensureModules(db, [m]);
      expect(await db.all("SELECT * FROM module_schema")).toEqual([{ module: "a", version: 1 }]);
    });

    test("a temporary schema that is as the migration found it is not a finding, rows or no rows", async () => {
      const { db, m } = viaTheConnection("CREATE TEMP TABLE zz (a TEXT)", (sqlite) =>
        sqlite.exec("INSERT INTO zz VALUES ('x')"),
      );
      await ensureModules(db, [m]);
      expect(await db.all("SELECT * FROM module_schema")).toEqual([{ module: "a", version: 1 }]);
    });
  });

  // The manager's own statements name their schema. A temporary table with the name of one of
  // the file's takes its place wherever the name is not qualified, so each of these has a
  // temporary table that stands in the way of one statement of the manager.
  describe("the statements of the manager name the schema they mean", () => {
    async function rawLegacyFile() {
      const sqlite = new DatabaseSync(":memory:");
      const db = sqlDbOver(sqlite);
      await firstBuildMigration(db);
      await seed(db);
      return { sqlite, db };
    }
    const tempNames = (sqlite: DatabaseSync) =>
      sqlite
        .prepare("SELECT type, name FROM sqlite_temp_master ORDER BY type, name")
        .all()
        .map((o) => `${String(o.type)} ${String(o.name)}`);
    const SHADOW_VERSIONS =
      "CREATE TEMP TABLE module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) WITHOUT ROWID";

    test("the versions are read from and written to the file's module_schema, not a temporary one", async () => {
      const sqlite = new DatabaseSync(":memory:");
      const db = sqlDbOver(sqlite);
      sqlite.exec(SHADOW_VERSIONS);
      const seen: number[] = [];
      const a = (version: number): SchemaModule => ({
        id: "a",
        version,
        async migrate(tx, from) {
          seen.push(from);
          if (from < 1) await tx.exec("CREATE TABLE a_items (id TEXT)");
          if (from < 2 && version >= 2) await tx.exec("ALTER TABLE a_items ADD COLUMN note TEXT");
        },
      });
      await ensureModules(db, [a(1)]);
      expect(sqlite.prepare("SELECT * FROM main.module_schema").all()).toEqual([
        { module: "a", version: 1 },
      ]);
      // The second start reads the file's versions, and moves from 1 to 2 and not from 0.
      await ensureModules(db, [a(2)]);
      expect(seen).toEqual([0, 1]);
      expect(sqlite.prepare("SELECT * FROM main.module_schema").all()).toEqual([
        { module: "a", version: 2 },
      ]);
      expect(sqlite.prepare("SELECT * FROM temp.module_schema").all()).toEqual([]);
    });

    test("the versions adopted from a file of the first build go to the file's module_schema", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec(SHADOW_VERSIONS);
      await ensureModules(db, [HOST_SCHEMA, statements(["ALTER TABLE trip ADD COLUMN note TEXT"])]);
      expect(sqlite.prepare("SELECT * FROM main.module_schema ORDER BY module").all()).toEqual([
        { module: "host", version: 1 },
        { module: "mileage", version: 2 },
      ]);
      expect(sqlite.prepare("SELECT * FROM temp.module_schema").all()).toEqual([]);
    });

    test("the versions that are compared are the file's, not a temporary table's", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec(SHADOW_VERSIONS);
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["UPDATE main.module_schema SET version = 9 WHERE module = 'host'"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(
        "module mileage changed the version recorded for host in module_schema (1 before, 9 after); only the manager writes the versions",
      );
    });

    test("the rows are counted in the file's table, not in a temporary one of the same name", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec("CREATE TEMP TABLE point AS SELECT * FROM main.point");
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["DELETE FROM main.point"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toMatch(
        /^module mileage lost rows of the table point \(3 before, 0 after\)/,
      );
      expect(await db.all("SELECT COUNT(*) AS n FROM main.point")).toEqual([{ n: 3 }]);
    });

    test("the columns are read from the file's table, not a temporary one of the same name", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec("CREATE TEMP TABLE trip (other TEXT)");
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["ALTER TABLE main.trip RENAME COLUMN purpose TO purpose2"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(
        "module mileage changed the column trip.purpose; a change that an older build cannot read raises the layout version instead",
      );
    });

    test("the foreign keys of a new table are read from the file's table", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec("CREATE TEMP TABLE mileage_x (a TEXT)");
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["CREATE TABLE main.mileage_x (a TEXT REFERENCES trip (client_trip_id))"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toMatch(
        /^module mileage made the table mileage_x with a foreign key to trip/,
      );
    });

    test("the indexes of a table are read from the file's table", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec("CREATE TEMP TABLE trip (purpose TEXT)");
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["CREATE UNIQUE INDEX main.mileage_u ON trip (client_trip_id, purpose)"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(
        "module mileage made the unique index mileage_u; an older build inserts rows that it would refuse",
      );
    });

    test("the keys of an index are read from the file's index, not a temporary one of the same name", async () => {
      const { sqlite, db } = await rawLegacyFile();
      sqlite.exec("CREATE TEMP TABLE zz (a TEXT); CREATE INDEX temp.mileage_i ON zz (a)");
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements(["CREATE INDEX main.mileage_i ON trip (lower(purpose))"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(
        "module mileage made the index mileage_i on an expression, which is evaluated on an older build's rows",
      );
    });

    test("the tables made for the look at the first layout are dropped from the file, and a temporary table of the name stays", async () => {
      const sqlite = new DatabaseSync(":memory:");
      const db = sqlDbOver(sqlite);
      sqlite.exec("CREATE TEMP TABLE trip (other TEXT)");
      await ensureModules(db, [HOST_SCHEMA, statements(["CREATE TABLE mileage_note (a TEXT)"])]);
      expect(tempNames(sqlite)).toEqual(["table trip"]);
      expect(
        sqlite
          .prepare("PRAGMA temp.table_info(trip)")
          .all()
          .map((c) => c.name),
      ).toEqual(["other"]);
    });
  });
});

describe("a pragma or an attached database does not outlive the migration that was refused", () => {
  test("PRAGMA locking_mode = EXCLUSIVE is refused, and a second connection can read the file", async () => {
    const file = tempDatabase();
    try {
      const one = file.connect();
      const two = file.connect();
      await one.db.exec("CREATE TABLE trip (a TEXT)");
      const error = await ensureModules(one.db, [
        {
          id: "a",
          version: 1,
          async migrate(tx) {
            await tx.exec("PRAGMA locking_mode = EXCLUSIVE");
            await tx.exec("CREATE TABLE a_items (id TEXT)");
          },
        },
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(`module a ran PRAGMA locking_mode; ${WHY.pragma}`);
      expect(two.sqlite.prepare("SELECT COUNT(*) AS n FROM trip").get()).toEqual({ n: 0 });
      expect(one.sqlite.prepare("PRAGMA locking_mode").get()).toEqual({ locking_mode: "normal" });
    } finally {
      file.close();
    }
  });

  test("ATTACH DATABASE is refused, and nothing is attached afterwards", async () => {
    const sqlite = new DatabaseSync(":memory:");
    const db = sqlDbOver(sqlite);
    const error = await ensureModules(db, [
      {
        id: "a",
        version: 1,
        migrate: (tx) => tx.exec("ATTACH DATABASE ':memory:' AS zz"),
      },
    ]).catch((e: unknown) => e);
    expect((error as Error).message).toBe(`module a ran text with the word ATTACH; ${WHY.attach}`);
    expect(await attachedTo(db)).toEqual([]);
  });

  test("the pragmas the connection is opened with are not touched by a migration that is accepted", async () => {
    const sqlite = new DatabaseSync(":memory:");
    const db = sqlDbOver(sqlite);
    const before = sqlite.prepare("PRAGMA foreign_keys").get();
    await ensureModules(db, [HOST_SCHEMA, MILEAGE_SCHEMA]);
    expect(sqlite.prepare("PRAGMA foreign_keys").get()).toEqual(before);
    expect(await attachedTo(db)).toEqual([]);
  });
});

describe("a default of the time of the insert in the text of an ALTER TABLE", () => {
  /** The text SQLite keeps for `point` after `ADD COLUMN c TEXT`, which a rebuild can be made to match. */
  async function afterAdd(): Promise<string> {
    const db = await emptyLegacyFile();
    await db.exec("ALTER TABLE point ADD COLUMN c TEXT");
    return (await db.one<{ sql: string }>("SELECT sql FROM sqlite_master WHERE name = 'point'"))!
      .sql;
  }

  // The column is added with the time of the insert as its default, and the table is then made
  // again, copied through, in the text that ADD COLUMN would have left with a plain column. The
  // state afterwards has no such default. SQLite refuses the first statement on a table that has
  // rows and accepts it on a table that has none, so this passed on an empty file and failed on a
  // phone with trips.
  async function copyThrough(word: string): Promise<SchemaModule> {
    const text = await afterAdd();
    return statements([
      `ALTER TABLE point ADD COLUMN c TEXT DEFAULT ${word}`,
      "CREATE TABLE mileage_tmp AS SELECT * FROM point",
      "DROP TABLE point",
      text,
      "INSERT INTO point SELECT * FROM mileage_tmp",
      "DROP TABLE mileage_tmp",
    ]);
  }
  const MESSAGE = (word: string) => `module mileage ran an ALTER TABLE with ${word}; ${WHY.alter}`;

  test.each(["CURRENT_TIME", "CURRENT_DATE", "CURRENT_TIMESTAMP"])(
    "%s: the copy-through is refused in a file with rows and in an empty one, with the same message",
    async (word) => {
      for (const start of [legacyFile, emptyLegacyFile]) {
        const db = await start();
        const message = await expectUntouched(db, async () =>
          ensureModules(db, [HOST_SCHEMA, await copyThrough(word)]),
        );
        expect(message).toBe(MESSAGE(word));
      }
    },
  );

  test("the same rebuild with a plain column is accepted, so that the refusal is the word's", async () => {
    for (const start of [legacyFile, emptyLegacyFile]) {
      const db = await start();
      const rows = await rowsOf(db);
      const text = await afterAdd();
      await ensureModules(db, [
        HOST_SCHEMA,
        statements([
          "ALTER TABLE point ADD COLUMN c TEXT DEFAULT 'x'",
          "CREATE TABLE mileage_tmp AS SELECT * FROM point",
          "DROP TABLE point",
          text,
          "INSERT INTO point SELECT * FROM mileage_tmp",
          "DROP TABLE mileage_tmp",
        ]),
      ]);
      expect((await rowsOf(db)).point.length).toBe(rows.point.length);
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    }
  });

  test("a table made in the same migration and then given such a column is refused, in a new file too", async () => {
    for (const start of [legacyFile, emptyLegacyFile, async () => memoryDb()]) {
      const db = await start();
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        statements([
          "CREATE TABLE mileage_x (id TEXT)",
          "ALTER TABLE mileage_x ADD COLUMN zz TEXT DEFAULT CURRENT_TIMESTAMP",
        ]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(MESSAGE("CURRENT_TIMESTAMP"));
    }
  });

  test("a default in a CREATE TABLE is not looked at: the table is new and its own", async () => {
    const db = await legacyFile();
    await ensureModules(db, [
      HOST_SCHEMA,
      statements(["CREATE TABLE mileage_y (id TEXT, zz TEXT DEFAULT CURRENT_TIMESTAMP)"]),
    ]);
    expect(await probeOlderBuild(db)).toEqual({ ok: true });
  });
});

describe("the manager's own table", () => {
  const a = (extra: Partial<SchemaModule>): SchemaModule => ({
    id: "a",
    version: 1,
    migrate: async () => undefined,
    ...extra,
  });

  test("a module whose id would own module_schema is refused, in a list of modules and before anything runs", async () => {
    expect(() => checkModules([a({ id: "module" })])).toThrow(
      new SchemaError(
        "module id module is the manager's: a module owns the tables named module_*, and module_schema is the manager's",
      ),
    );
    const db = memoryDb();
    const error = await ensureModules(db, [
      a({ id: "other", migrate: (tx) => tx.exec("CREATE TABLE other_t (a TEXT)") }),
      a({ id: "module", migrate: (tx) => tx.exec("ALTER TABLE module_schema ADD COLUMN x TEXT") }),
    ]).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(SchemaError);
    expect(await objectsOf(db)).toEqual([]);
  });

  test("a module may not claim module_schema as a legacy table", () => {
    expect(() => checkModules([a({ legacyTables: ["module_schema"] })])).toThrow(
      new SchemaError("module_schema is the manager's"),
    );
  });

  test("ids that only begin like it are modules", () => {
    for (const id of ["mod", "modul", "modules", "module2", "schema", "xmodule"]) {
      expect(() => checkModules([a({ id })]), id).not.toThrow();
    }
  });
});

describe("the text that the look at the first layout runs", () => {
  // In a new file the manager makes the tables of the first layout that a module declares, for the
  // length of the look, with the text the module declares. That text is the module's, and goes
  // through the guard like a migration.
  const declaring = (definition: string): SchemaModule => ({
    id: "a",
    version: 1,
    legacyTables: ["a_t"],
    legacyDefinitions: { a_t: definition },
    migrate: (tx) => tx.exec("CREATE TABLE IF NOT EXISTS a_t (x TEXT)"),
  });

  test.each([
    [
      "COMMIT after the table",
      "CREATE TABLE IF NOT EXISTS a_t (x TEXT); COMMIT",
      "module a ran COMMIT; the manager holds the transaction and takes all of it back when anything fails, so a migration may not begin, end or nest one (BEGIN, COMMIT, END, ROLLBACK, SAVEPOINT, RELEASE)",
    ],
    [
      "a ROLLBACK conflict clause",
      "CREATE TABLE IF NOT EXISTS a_t (x TEXT NOT NULL ON CONFLICT ROLLBACK)",
      `module a ran text with the word ROLLBACK; ${WHY.rollback}`,
    ],
    [
      "a temporary table",
      "CREATE TEMP TABLE a_t (x TEXT)",
      `module a ran text with the word TEMP; ${WHY.temp}`,
    ],
  ])(
    "a definition with %s is refused, and nothing is left in the file",
    async (_, text, message) => {
      const db = memoryDb();
      const error = await ensureModules(db, [declaring(text)]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(message);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    },
  );

  test("a definition that is not run, because the file has the table, is not looked at", async () => {
    const db = memoryDb();
    await db.exec("CREATE TABLE a_t (x TEXT); PRAGMA user_version = 1");
    await ensureModules(db, [declaring("CREATE TABLE IF NOT EXISTS a_t (x TEXT); COMMIT")]);
    expect(await db.all("SELECT * FROM module_schema")).toEqual([{ module: "a", version: 1 }]);
  });
});

// The function pragma_optimize runs PRAGMA optimize, which can run ANALYZE and write the table
// sqlite_stat1 into the file. The lint does not look at tables of SQLite's own (the snapshot leaves
// out every name that begins with sqlite_), and the word PRAGMA is not in the text, so it is
// refused by its name, as a bare word and as a name in double quotes, backticks or brackets, in
// any case. The other pragma_* functions are not refused (the sweep below says what that rests on).
describe("the function pragma_optimize", () => {
  const OPTIMIZE_WORD = { ran: "text with the word PRAGMA_OPTIMIZE", why: WHY.optimize };
  const OPTIMIZE_NAME = { ran: "text with the name pragma_optimize", why: WHY.optimize };

  test.each([
    "SELECT * FROM pragma_optimize",
    "SELECT * FROM pragma_optimize(0x10002)",
    "SELECT * FROM pragma_optimize AS p",
    "SELECT 1 FROM (SELECT * FROM pragma_optimize)",
    "SELECT * FROM main.pragma_optimize",
    "SELECT * FROM a_t, pragma_optimize",
    "INSERT INTO a_t SELECT * FROM pragma_optimize",
  ])("%s is refused, in every casing and behind every prefix", (form) => {
    for (const casing of casings("pragma_optimize")) {
      for (const prefix of PREFIXES) {
        const sql = prefix + form.replace("pragma_optimize", casing);
        expect(forbiddenInMigration(sql), sql).toEqual(OPTIMIZE_WORD);
      }
    }
  });

  test.each(['"pragma_optimize"', "`pragma_optimize`", "[pragma_optimize]", '"Pragma_OPTIMIZE"'])(
    "the name %s is refused, with and without an argument and behind every prefix",
    (name) => {
      for (const prefix of PREFIXES) {
        for (const sql of [
          `${prefix}SELECT * FROM ${name}`,
          `${prefix}SELECT * FROM ${name}(0x10002)`,
          `${prefix}SELECT * FROM main.${name}`,
        ]) {
          expect(forbiddenInMigration(sql), sql).toEqual(OPTIMIZE_NAME);
        }
      }
    },
  );

  // The positive controls: what is not the function is not refused.
  test.each([
    "SELECT 'pragma_optimize'",
    "INSERT INTO a_t (x) VALUES ('call pragma_optimize later')",
    "SELECT 1 -- pragma_optimize\n",
    "/* pragma_optimize */ SELECT 1",
    "SELECT 1 AS pragma_optimized",
    "SELECT 1 AS my_pragma_optimize",
    "SELECT * FROM pragma_optimize_x",
    "SELECT * FROM pragma_table_info('trip')",
    "SELECT * FROM pragma_index_list('trip')",
    "SELECT user_version FROM pragma_user_version",
    "SELECT * FROM pragma_integrity_check",
    "SELECT * FROM pragma_foreign_key_check",
    "SELECT * FROM pragma_database_list",
    "SELECT * FROM pragma_pragma_list WHERE name = 'optimize'",
  ])("%s is not refused", (sql) => {
    expect(forbiddenInMigration(sql)).toBeNull();
  });

  // A limit, pinned: SQLite also reads a single-quoted string in the place of a table name, and
  // this guard reads that as a string, which is not refused (a string with the name in it is no
  // harm anywhere else). So this text passes the guard and writes sqlite_stat1; so does the
  // statement ANALYZE, which is not refused either. Both are planner statistics and the older
  // build does not mind them (the probe passes), but they are a write that the guard and the
  // checks afterwards neither refuse nor report.
  test.each(["SELECT * FROM 'pragma_optimize'", "ANALYZE"])(
    "%s passes the guard, writes sqlite_stat1 and is not reported (a limit)",
    async (sql) => {
      expect(forbiddenInMigration(sql)).toBeNull();
      const db = await legacyFile();
      await ensureModules(db, [HOST_SCHEMA, mileageAt2((tx) => tx.exec(sql))]);
      expect((await objectsOf(db)).map((o) => o.name)).toContain("sqlite_stat1");
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    },
  );

  describe.each(METHODS)("through %s, on a file of the first build with rows", (method) => {
    test.each([
      ["SELECT * FROM pragma_optimize", OPTIMIZE_WORD.ran],
      ['SELECT * FROM "pragma_optimize"', OPTIMIZE_NAME.ran],
    ])("%s is refused before it runs and nothing is written", async (sql, was) => {
      const db = await legacyFile();
      const message = await expectUntouched(db, () =>
        ensureModules(db, [
          HOST_SCHEMA,
          mileageAt2(async (tx) => {
            await call(tx, method, sql);
          }),
        ]),
      );
      expect(message).toBe(`module mileage ran ${was}; ${WHY.optimize}`);
      expect((await objectsOf(db)).map((o) => o.name)).not.toContain("sqlite_stat1");
    });
  });

  test("a refusal that the migration swallows leaves the function unrun, and the manager judges the rest", async () => {
    const db = await legacyFile();
    await ensureModules(db, [
      HOST_SCHEMA,
      mileageAt2(async (tx) => {
        await tx.all("SELECT * FROM pragma_optimize").catch(() => undefined);
      }),
    ]);
    expect((await objectsOf(db)).map((o) => o.name)).not.toContain("sqlite_stat1");
  });

  // What "the other pragma_* functions only read" rests on, as far as it goes: every pragma that
  // SQLite lists, and that this SQLite offers as a function, is called with 15 shapes of argument
  // inside a transaction, and the catalogue, the temporary schema and the value of every pragma
  // that can be read without acting are compared before and after. Only optimize changes anything.
  // This is a sweep of one build of SQLite (the one Node has), not a proof for any other function
  // or any other build; a build that has a function with an effect makes this test fail.
  test("of the pragma_* functions of this SQLite only pragma_optimize changes the file or the connection", () => {
    const make = () => {
      const sqlite = new DatabaseSync(":memory:");
      sqlite.exec(
        "CREATE TABLE t (a TEXT, b TEXT); CREATE INDEX t_a ON t (a); INSERT INTO t VALUES ('x', 'y'), ('x', 'z')",
      );
      return sqlite;
    };
    const probe = make();
    const pragmas = probe
      .prepare("SELECT name FROM pragma_pragma_list ORDER BY name")
      .all()
      .map((r) => String(r.name));
    const functions = pragmas.filter((name) => {
      try {
        probe.prepare(`SELECT * FROM pragma_${name}`).all();
        return true;
      } catch (e) {
        return !/no such table/i.test(String(e));
      }
    });
    expect(functions).toContain("optimize");
    expect(functions).toContain("table_info");
    expect(functions.length).toBeGreaterThan(40);
    expect(functions.length).toBeLessThanOrEqual(pragmas.length);

    // A pragma that does something when it is only read is not part of the state that is compared.
    const acts = new Set([
      "optimize",
      "shrink_memory",
      "wal_checkpoint",
      "incremental_vacuum",
      "stats",
      "lock_status",
      "key",
      "rekey",
      "hexkey",
      "hexrekey",
      "textkey",
      "textrekey",
      "activate_extensions",
      "parser_trace",
      "vdbe_trace",
      "vdbe_listing",
      "vdbe_addoptrace",
      "vdbe_debug",
      "vdbe_eqp",
      "sql_trace",
      "temp_store_directory",
      "data_store_directory",
      "lock_proxy_file",
      "integrity_check",
      "quick_check",
      "foreign_key_check",
      "database_list",
      "module_list",
      "function_list",
      "pragma_list",
      "collation_list",
      "compile_options",
      "table_list",
    ]);
    const compared = pragmas.filter((name) => !acts.has(name));
    // The numbers 8.7.1 quotes were counted on this build of SQLite; another build is swept, not refused.
    const version = String(
      (probe.prepare("SELECT sqlite_version() AS v").get() as { v: string }).v,
    );
    if (version === "3.54.0") {
      expect([pragmas.length, functions.length, compared.length]).toEqual([67, 59, 51]);
    }
    const state = (sqlite: DatabaseSync): Record<string, string> => {
      const out: Record<string, string> = {};
      for (const name of compared) {
        try {
          out[name] = JSON.stringify(sqlite.prepare(`PRAGMA ${name}`).all());
        } catch (e) {
          out[name] = `error ${String(e)}`;
        }
      }
      out["catalogue"] = JSON.stringify(
        sqlite.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY name").all(),
      );
      out["temporary"] = JSON.stringify(sqlite.prepare("SELECT * FROM sqlite_temp_master").all());
      out["databases"] = JSON.stringify(sqlite.prepare("PRAGMA database_list").all());
      return out;
    };
    const shapes = [
      "",
      "(0)",
      "(1)",
      "(2)",
      "(-1)",
      "('main')",
      "('temp')",
      "('t')",
      "('main', 't')",
      "('t', 'main')",
      "(0x10002)",
      "(NULL)",
      "('t_a')",
      "('exclusive')",
      "(1, 2)",
    ];
    expect(shapes).toHaveLength(15);
    const changed: string[] = [];
    let calls = 0;
    for (const name of functions) {
      const sqlite = make();
      sqlite.exec("BEGIN IMMEDIATE");
      state(sqlite); // the first read opens what a read opens
      const before = state(sqlite);
      for (const shape of shapes) {
        const sql = `SELECT * FROM pragma_${name}${shape}`;
        // The guard refuses this one function by its name and lets the others through.
        expect(forbiddenInMigration(sql) === null, sql).toBe(name !== "optimize");
        try {
          sqlite.prepare(sql).all();
        } catch {
          // Many shapes are wrong for a function (too many arguments, no such schema).
        }
        calls++;
        const after = state(sqlite);
        if (Object.keys(before).some((key) => before[key] !== after[key])) {
          if (!changed.includes(name)) changed.push(name);
        }
      }
      sqlite.exec("ROLLBACK");
    }
    expect(calls).toBe(functions.length * shapes.length);
    expect(changed).toEqual(["optimize"]);
  });
});

// Seven statements of the manager name `main.` and the tests above cannot show that they have to:
// removing the `main.` from them changes no result, because SQLite itself reads the unqualified
// form as the file's even where a temporary object has the name. The other ten (the versions, the
// counts, the columns, the foreign keys, the indexes, the keys of an index, the drops, the two
// writes of a version) are shown above with a temporary table of the same name. This is the test
// of the fact that the seven rest on, one statement at a time, in the text the manager uses: if a
// build of SQLite read them otherwise, this fails and the `main.` becomes necessary.
describe("the statements of the manager for which main. is for the reader", () => {
  const raw = () => {
    const sqlite = new DatabaseSync(":memory:");
    sqlite.exec(
      "CREATE TABLE trip (client_trip_id TEXT); PRAGMA main.user_version = 1; PRAGMA temp.user_version = 77",
    );
    return sqlite;
  };
  const SHADOW =
    "CREATE TEMP TABLE module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) WITHOUT ROWID";

  test("the layout is the file's: PRAGMA user_version read (readState and snapshot) and set (the end of ensureModules)", () => {
    const sqlite = raw();
    expect(sqlite.prepare("PRAGMA user_version").get()).toEqual({ user_version: 1 });
    sqlite.exec("PRAGMA user_version = 2");
    expect(sqlite.prepare("PRAGMA main.user_version").get()).toEqual({ user_version: 2 });
    expect(sqlite.prepare("PRAGMA temp.user_version").get()).toEqual({ user_version: 77 });
  });

  test("the catalogue is the file's: readState, the snapshot and the look read sqlite_master, which a temporary object cannot take the place of", () => {
    const sqlite = raw();
    sqlite.exec(SHADOW);
    // readState: the file has no module_schema, the temporary schema has one.
    expect(
      sqlite
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'module_schema'")
        .all(),
    ).toEqual([]);
    // The snapshot and the look: only the file's objects are listed.
    expect(
      sqlite
        .prepare(
          "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
        )
        .all()
        .map((o) => o.name),
    ).toEqual(["trip"]);
    expect(
      sqlite
        .prepare("SELECT name FROM sqlite_master")
        .all()
        .map((o) => o.name),
    ).toEqual(["trip"]);
    // Nothing can be made to take the name: it is reserved.
    expect(() => sqlite.exec("CREATE TEMP TABLE sqlite_master (a TEXT)")).toThrow(/reserved/);
  });

  test("module_schema is made in the file, also where a temporary table has the name", () => {
    const sqlite = raw();
    sqlite.exec(SHADOW);
    sqlite.exec(
      "CREATE TABLE IF NOT EXISTS module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) WITHOUT ROWID",
    );
    expect(
      sqlite
        .prepare("SELECT name FROM main.sqlite_master WHERE name = 'module_schema'")
        .all()
        .map((o) => o.name),
    ).toEqual(["module_schema"]);
  });
});

// A quoted name or a string is not a word. It matters where the reader looks for a word at the
// start of a statement or after PRAGMA: a name there is not the word, so the guard does not take it
// for one (SQLite refuses such a text itself, and says so), and `PRAGMA "user_version"` is not the
// pragma that may be used (refused, as what is not on the list is).
describe("a quoted name or a string where the reader looks for a word", () => {
  const WORDS = ["BEGIN", "COMMIT", "END", "ROLLBACK", "SAVEPOINT", "RELEASE", "ALTER", "PRAGMA"];

  test.each(WORDS)(
    "%s as a quoted name or a string at the start of a statement is not the word",
    (word) => {
      for (const written of [`"${word}"`, `[${word}]`, `\`${word}\``, `'${word}'`]) {
        for (const sql of [written, `SELECT 1; ${written}`, `${written} CURRENT_TIME`]) {
          expect(transactionControl(sql), sql).toBeNull();
          expect(forbiddenInMigration(sql), sql).toBeNull();
        }
      }
    },
  );

  test.each(["BEGIN", "COMMIT", "ROLLBACK"])(
    "a migration that runs \"%s\" gets SQLite's syntax error and not the guard's refusal",
    async (word) => {
      const db = memoryDb();
      const error = await ensureModules(db, [
        { id: "a", version: 1, migrate: (tx) => tx.exec(`"${word}"`) },
      ]).catch((e: unknown) => e);
      expect(error).toBeInstanceOf(Error);
      expect(error).not.toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(/syntax error/);
    },
  );

  test('PRAGMA user_version is the one pragma that may be used, and PRAGMA "user_version" is not it', () => {
    expect(forbiddenInMigration("PRAGMA user_version")).toBeNull();
    expect(forbiddenInMigration("PRAGMA user_version = 1")).toBeNull();
    for (const sql of ['PRAGMA "user_version"', "PRAGMA [user_version]", "PRAGMA 'user_version'"]) {
      expect(forbiddenInMigration(sql)?.why, sql).toBe(WHY.pragma);
    }
  });

  test("a quoted name or a string after CREATE is not TEMP, TEMPORARY or TRIGGER", () => {
    // `CREATE "TEMP" TABLE` is not a temporary table (SQLite refuses it as a syntax error), and
    // `CREATE "TRIGGER" ...` is not a trigger whose body is read to its END: the text is split at
    // the semicolons, and the END that follows one is the first word of a statement.
    expect(ran('CREATE "TEMP" TABLE a_t (x TEXT)')).toBeNull();
    expect(transactionControl('CREATE "TRIGGER" x BEGIN SELECT 1; END')).toEqual({ word: "END" });
    // The same for the words TEMP and TEMPORARY that may stand between CREATE and TRIGGER: a quoted
    // name there is not one, so the text is not read as a trigger (SQLite refuses it as a syntax error).
    expect(transactionControl('CREATE "TEMP" TRIGGER x BEGIN SELECT 1; END')).toEqual({
      word: "END",
    });
    expect(transactionControl('CREATE "TEMPORARY" TRIGGER x BEGIN SELECT 1; END')).toEqual({
      word: "END",
    });
    expect(transactionControl("CREATE TEMP TRIGGER x BEGIN SELECT 1; END")).toBeNull();
    expect(transactionControl("CREATE TEMPORARY TRIGGER x BEGIN SELECT 1; END")).toBeNull();
    // A string in those places is not the word either.
    for (const between of ["'TEMP'", "'TEMPORARY'"]) {
      expect(transactionControl(`CREATE ${between} TRIGGER x BEGIN SELECT 1; END`)).toEqual({
        word: "END",
      });
    }
    expect(transactionControl("CREATE 'TRIGGER' x BEGIN SELECT 1; END")).toEqual({ word: "END" });
  });
});

// Limit 4 of 8.7.1: connection state that no refused word names, and another handle to the
// database. A migration is code. This one closes over the handle the test passes in (the app has
// nothing like it) and registers a SQL function through it: no text is run that the guard could
// read, the catalogue does not change, the manager accepts the migration, and the function is on
// the connection afterwards. It is there also after a migration that the manager refused and took
// back, because the transaction does not hold it. Through the text of the guarded transaction no
// state of this kind is known (the words above are the ones that were found).
describe("a limit: connection state that no word names, set through another handle", () => {
  function viaTheHandle(after?: (tx: SqlTx) => Promise<void>) {
    const sqlite = new DatabaseSync(":memory:");
    const db = sqlDbOver(sqlite);
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_items (id TEXT)");
        sqlite.function("zz_marker", () => 42);
        await after?.(tx);
      },
    };
    return { sqlite, db, m };
  }

  test("a function registered through the handle is on the connection after an accepted migration, and nothing says so", async () => {
    const { sqlite, db, m } = viaTheHandle();
    expect(() => sqlite.prepare("SELECT zz_marker() AS v").get()).toThrow(/no such function/);
    await ensureModules(db, [m]);
    expect(sqlite.prepare("SELECT zz_marker() AS v").get()).toEqual({ v: 42 });
    expect(await db.all("SELECT * FROM module_schema")).toEqual([{ module: "a", version: 1 }]);
  });

  test("it is there also after a migration that was refused and taken back", async () => {
    const { sqlite, db, m } = viaTheHandle((tx) =>
      tx.exec("CREATE TRIGGER a_g AFTER INSERT ON a_items BEGIN SELECT 1; END"),
    );
    const error = await ensureModules(db, [m]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect(await objectsOf(db)).toEqual([]);
    expect(await layout(db)).toBe(0);
    expect(sqlite.prepare("SELECT zz_marker() AS v").get()).toEqual({ v: 42 });
  });
});

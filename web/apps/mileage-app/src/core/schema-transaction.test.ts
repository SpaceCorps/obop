import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "./host-state.ts";
import { SchemaError, ensureModules, transactionControl, type SchemaModule } from "./schema.ts";
import type { SqlDb, SqlTx } from "./sql.ts";
import { MILEAGE_SCHEMA } from "./sql-store.ts";
import { emptyLegacyFile, legacyFile } from "../test-support/legacy-file.ts";
import { memoryDb } from "../test-support/memory-db.ts";
import { probeOlderBuild } from "../test-support/older-build-probe.ts";
import { tempDatabase } from "../test-support/temp-db.ts";

// A migration runs inside the one transaction that the manager holds for all the modules, and the
// manager takes all of it back when anything fails. This file holds the two things that keep a
// migration from ending that transaction (the guard that refuses the statement before it runs, and
// the check afterwards that the transaction is still the one that was open), and the rule that the
// versions in `module_schema` are the manager's alone. It also pins two limits of the check that
// 8.7.1 states, because a limit that is written down is one a test holds.

const KEYWORDS = ["BEGIN", "COMMIT", "END", "ROLLBACK", "SAVEPOINT", "RELEASE"] as const;

/** A statement that begins with the word, as it is usually written. */
const STATEMENTS: Record<(typeof KEYWORDS)[number], string[]> = {
  BEGIN: ["BEGIN", "BEGIN IMMEDIATE", "BEGIN EXCLUSIVE TRANSACTION"],
  COMMIT: ["COMMIT", "COMMIT TRANSACTION"],
  END: ["END", "END TRANSACTION"],
  ROLLBACK: ["ROLLBACK", "ROLLBACK TO x", "ROLLBACK TRANSACTION TO SAVEPOINT x"],
  SAVEPOINT: ["SAVEPOINT x"],
  RELEASE: ["RELEASE x", "RELEASE SAVEPOINT x"],
};

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

/** Writes a statement in a casing: only its first word changes. */
const written = (statement: string, casing: string): string => {
  const [, ...rest] = statement.split(" ");
  return [casing, ...rest].join(" ");
};

describe("the reader of a migration's text for statements that control a transaction", () => {
  test.each(KEYWORDS)("finds %s as the first word of a statement, in every casing", (keyword) => {
    for (const statement of STATEMENTS[keyword]) {
      for (const casing of casings(keyword)) {
        const text = written(statement, casing);
        // On its own, after a comment of either kind, after one statement and after several, with
        // the statement before it joined by a semicolon with or without a space, and with a
        // semicolon after it.
        for (const sql of [
          text,
          `${text};`,
          `/* a */ ${text}`,
          `-- a\n${text}`,
          `/* ; COMMIT */ -- ; END\n${text}`,
          `SELECT 1; ${text}`,
          `SELECT 1;${text}`,
          `SELECT 1;;;${text}`,
          `SELECT 1; /* c */ -- d\n ${text}`,
          `SELECT 1; SELECT 'x;y'; SELECT [a;b]; ${text}`,
        ]) {
          expect(transactionControl(sql), sql).toEqual({ word: keyword });
        }
      }
    }
  });

  test("finds the first one when there are several", () => {
    expect(transactionControl("SELECT 1; ROLLBACK; COMMIT")).toEqual({ word: "ROLLBACK" });
    expect(transactionControl("SELECT 1; COMMIT; ROLLBACK")).toEqual({ word: "COMMIT" });
  });

  test.each([
    "",
    "   ",
    "SELECT 1",
    "SELECT 1; SELECT 2",
    "SELECT 1;;; SELECT 2;",
    "CREATE TABLE a_t (id TEXT)",
    // A word is a statement only where a statement starts.
    "CREATE TABLE a_t (begin TEXT, end TEXT, commit TEXT, rollback TEXT, release TEXT, savepoint TEXT)",
    "INSERT INTO a_t (end) VALUES (1)",
    "UPDATE a_t SET end = 1, begin = 2 WHERE commit = 3",
    "SELECT end FROM a_t WHERE begin = 1",
    // In a string, a quoted name, a blob or a comment, it is not a word at all.
    "SELECT 'COMMIT'",
    "SELECT 'x'; SELECT 'COMMIT'",
    "SELECT ';COMMIT'",
    "SELECT 'it''s; COMMIT'",
    'SELECT "COMMIT"',
    'SELECT "a"";COMMIT"',
    "SELECT [END]",
    "SELECT `BEGIN`",
    "SELECT x'434f4d4d4954'",
    // A name or a string at the start of a statement is not a word either.
    "'COMMIT'",
    '"COMMIT"',
    "[COMMIT]",
    "`COMMIT`",
    "SELECT 1; 'END'",
    'SELECT 1; "ROLLBACK"',
    "SELECT 1; [BEGIN]",
    "SELECT 1 /* ; COMMIT */",
    "SELECT 1 -- ; COMMIT",
    "SELECT 1 -- ; COMMIT\n; SELECT 2",
    "SELECT 1 --COMMIT\n",
    "/* COMMIT */",
    "-- COMMIT",
    // A word that only begins like one is another word.
    "SELECT 1; COMMITTED",
    "SELECT 1; ENDING",
    "SELECT 1; BEGINNER",
    "SELECT 1; RELEASED",
    "SELECT 1; ROLLBACKS",
    "SELECT 1; SAVEPOINTS",
    "SELECT 1; END_X",
    // EXPLAIN does not run what follows it.
    "EXPLAIN COMMIT",
  ])("does not find one in %j", (sql) => {
    expect(transactionControl(sql)).toBeNull();
  });

  // The body of a trigger is statements inside a statement: `CREATE TRIGGER ... BEGIN ...; END`.
  // The reader goes to the END that follows a `;` and comes before a `;` or the end of the text.
  test.each([
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT RAISE(ABORT, 'no'); END",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT RAISE(ABORT, 'no'); END;",
    "create trigger t before insert on a_t begin select 1; end",
    "CREATE TEMP TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END",
    "CREATE TEMPORARY TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END",
    "CREATE TRIGGER IF NOT EXISTS t AFTER INSERT ON a_t BEGIN SELECT 1; SELECT 2; END",
    // A CASE ends with END too, and a column can be called end.
    "CREATE TRIGGER t AFTER INSERT ON a_t BEGIN UPDATE a_t SET x = CASE WHEN 1 THEN 2 END; SELECT 1; END",
    "CREATE TRIGGER t AFTER INSERT ON a_t BEGIN UPDATE a_t SET x = CASE WHEN 1 THEN 2 END; END",
    "CREATE TRIGGER t BEFORE INSERT ON a_t WHEN NEW.end = 1 BEGIN SELECT 1; END",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 'END; COMMIT'; END",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN /* ; END; */ SELECT 1; END",
    // Several triggers, and other statements around them.
    "SELECT 1; CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END; SELECT 2",
    "CREATE TRIGGER a BEFORE INSERT ON a_t BEGIN SELECT 1; END; CREATE TRIGGER b BEFORE UPDATE ON a_t BEGIN SELECT 2; END",
  ])("reads a trigger to its end: %j", (sql) => {
    expect(transactionControl(sql)).toBeNull();
  });

  test.each<[string, string]>([
    ["CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END; COMMIT", "COMMIT"],
    ["CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END;\nrollback", "ROLLBACK"],
    ["CREATE TEMP TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END; END", "END"],
    [
      "CREATE TRIGGER a BEFORE INSERT ON a_t BEGIN SELECT 1; END; CREATE TRIGGER b BEFORE UPDATE ON a_t BEGIN SELECT 2; END; COMMIT",
      "COMMIT",
    ],
    [
      "SELECT 1; CREATE TRIGGER t AFTER INSERT ON a_t BEGIN UPDATE a_t SET x = CASE WHEN 1 THEN 2 END; END; BEGIN",
      "BEGIN",
    ],
  ])("finds the statement after a trigger: %j", (sql, word) => {
    expect(transactionControl(sql)).toEqual({ word });
  });

  // A text that cannot be split with certainty is not said to be safe.
  test.each([
    "SELECT 'abc",
    'SELECT "abc',
    "SELECT `abc",
    "SELECT [abc",
    "SELECT x'ab",
    "SELECT 1 /* open",
    "SELECT 1; COMMIT /* open",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1;",
    "CREATE TEMP TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; SELECT 2",
    // END has to be followed by a semicolon or by nothing, or it is not the end of the trigger.
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END COMMIT",
    "CREATE TRIGGER t BEFORE INSERT ON a_t BEGIN SELECT 1; END x",
  ])("says it cannot read %j", (sql) => {
    expect(transactionControl(sql)).toBe("unreadable");
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

describe("a migration that tries to begin, end or nest the manager's transaction", () => {
  // Each method that runs text refuses it, before it runs: the table the migration made first is
  // taken back with the rest, and the migration does not go on.
  describe.each(METHODS)("through %s", (method) => {
    test.each(KEYWORDS)("%s is refused before it runs", async (keyword) => {
      const sql = STATEMENTS[keyword][0]!;
      const db = memoryDb();
      const m = running("a", method, sql);
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(
        `module a ran ${keyword}; the manager holds the transaction and takes all of it back when anything fails, so a migration may not begin, end or nest one (BEGIN, COMMIT, END, ROLLBACK, SAVEPOINT, RELEASE)`,
      );
      expect(m.reached).toEqual([]);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });
  });

  test("every casing, with a comment before it and after another statement, is refused through the manager", async () => {
    for (const casing of casings("commit")) {
      for (const sql of [
        casing,
        `/* x */ ${casing}`,
        `-- x\n${casing}`,
        `SELECT 1; ${casing}`,
        `SELECT 1;${casing};`,
      ]) {
        const db = memoryDb();
        const error = await ensureModules(db, [running("a", "exec", sql)]).catch((e: unknown) => e);
        expect(error, sql).toBeInstanceOf(SchemaError);
        expect((error as Error).message, sql).toMatch(/^module a ran COMMIT;/);
        expect(await objectsOf(db), sql).toEqual([]);
      }
    }
  });

  test("a text that cannot be split is refused with a message of its own", async () => {
    const db = memoryDb();
    const m = running("a", "exec", "SELECT 'a;COMMIT");
    const error = await ensureModules(db, [m]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toBe(
      "module a ran text that cannot be split into statements with certainty, so it cannot be shown to leave the manager's transaction alone",
    );
    expect(m.reached).toEqual([]);
    expect(await objectsOf(db)).toEqual([]);
  });

  test("the module's id is in the message, and a refusal takes the earlier modules back too", async () => {
    const db = memoryDb();
    const first: SchemaModule = {
      id: "first",
      version: 1,
      migrate: (tx) => tx.exec("CREATE TABLE first_items (id TEXT)"),
    };
    const error = await ensureModules(db, [first, running("second", "run", "COMMIT")]).catch(
      (e: unknown) => e,
    );
    expect((error as Error).message).toMatch(/^module second ran COMMIT;/);
    expect(await objectsOf(db)).toEqual([]);
    expect(await layout(db)).toBe(0);
  });

  test("a migration that catches the refusal and goes on has still run nothing of it", async () => {
    const db = memoryDb();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_items (id TEXT)");
        await tx.exec("COMMIT").catch(() => undefined);
        await tx.exec("CREATE TABLE a_more (id TEXT)");
      },
    };
    await ensureModules(db, [m]);
    expect((await objectsOf(db)).map((o) => o.name)).toEqual([
      "a_items",
      "a_more",
      "module_schema",
    ]);
  });

  // What a person writes in good faith: a string or a comment with the word, a column called by it.
  test("the words in a string, a comment or a name do no harm", async () => {
    const db = memoryDb();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_log (note TEXT)");
        await tx.run("INSERT INTO a_log (note) VALUES ('COMMIT; END; ROLLBACK')");
        await tx.run("INSERT INTO a_log (note) VALUES ('it''s; COMMIT')");
        await tx.exec("/* COMMIT; */ INSERT INTO a_log (note) VALUES ('-- ; COMMIT')");
        await tx.exec('CREATE TABLE a_words ("commit" TEXT, [end] TEXT, begin TEXT)');
        expect(await tx.all("SELECT note FROM a_log WHERE note LIKE '%COMMIT%'")).toHaveLength(3);
        expect(await tx.one("SELECT 'BEGIN' AS word")).toEqual({ word: "BEGIN" });
      },
    };
    await ensureModules(db, [m]);
    expect(await db.all("SELECT note FROM a_log ORDER BY note")).toHaveLength(3);
  });

  test("a trigger is read to its end, and is refused for being a trigger, with the message of the lint", async () => {
    const db = memoryDb();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_items (id TEXT)");
        await tx.exec(
          "CREATE TRIGGER a_guard BEFORE INSERT ON a_items BEGIN SELECT RAISE(ABORT, 'no'); END",
        );
      },
    };
    const error = await ensureModules(db, [m]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(
      /^module a made the trigger a_guard; it would run on the statements of an older build/,
    );
  });

  test("a COMMIT after a trigger in the same text is refused as a COMMIT", async () => {
    const db = memoryDb();
    const m = running(
      "a",
      "exec",
      "CREATE TRIGGER a_guard BEFORE INSERT ON a_items BEGIN SELECT RAISE(ABORT, 'no'); END; COMMIT",
    );
    const error = await ensureModules(db, [m]).catch((e: unknown) => e);
    expect((error as Error).message).toMatch(/^module a ran COMMIT;/);
    expect(await objectsOf(db)).toEqual([]);
  });

  // The four files of the verifier's reports: a module of the app at version 2 on a file of the
  // first build, with rows. Each used to be refused with some message and still lose its work.
  describe("on a file of the first build, with rows", () => {
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

    test.each<[string, string[], RegExp]>([
      ["rows deleted, then COMMIT", ["DELETE FROM point", "COMMIT"], /^module mileage ran COMMIT;/],
      [
        "a table dropped, then COMMIT",
        ["DROP TABLE trip", "COMMIT"],
        /^module mileage ran COMMIT;/,
      ],
      [
        "ROLLBACK, then a table dropped",
        ["ROLLBACK", "DROP TABLE trip"],
        /^module mileage ran ROLLBACK;/,
      ],
      [
        "a column added, then COMMIT",
        ["ALTER TABLE trip ADD COLUMN x TEXT", "COMMIT"],
        /^module mileage ran COMMIT;/,
      ],
      ["END", ["DELETE FROM batch", "END TRANSACTION"], /^module mileage ran END;/],
      [
        "a savepoint, a change and a release of it",
        ["SAVEPOINT s", "DELETE FROM point", "RELEASE s"],
        /^module mileage ran SAVEPOINT;/,
      ],
      [
        "a change in a text with a COMMIT at its end",
        ["DELETE FROM point; COMMIT"],
        /^module mileage ran COMMIT;/,
      ],
    ])("%s leaves the file as it was", async (_, statements, message) => {
      const db = await legacyFile();
      const before = { objects: await objectsOf(db), rows: await rowsOf(db) };
      const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statements)]).catch(
        (e: unknown) => e,
      );
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toMatch(message);
      expect(await objectsOf(db)).toEqual(before.objects);
      expect(await rowsOf(db)).toEqual(before.rows);
      expect(await layout(db)).toBe(1);
      expect(
        before.objects.some((o) => o.name === "module_schema"),
        "the file had no module_schema",
      ).toBe(false);
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    });

    test("and so does a new file", async () => {
      const db = memoryDb();
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        mileageAt2(["DROP TABLE trip", "COMMIT"]),
      ]).catch((e: unknown) => e);
      expect((error as Error).message).toMatch(/^module mileage ran COMMIT;/);
      expect(await objectsOf(db)).toEqual([]);
      expect(await layout(db)).toBe(0);
    });

    test("and a migration without any of it goes through, as before", async () => {
      const db = await legacyFile();
      await ensureModules(db, [HOST_SCHEMA, mileageAt2(["ALTER TABLE trip ADD COLUMN x TEXT"])]);
      expect(await probeOlderBuild(db)).toEqual({ ok: true });
    });
  });
});

// A migration is handed a transaction that refuses those statements. A migration that has another
// way to the connection (a closure over the driver, which nothing in the app does) is outside the
// guard, and this is the check that finds it afterwards: the manager sets a savepoint before the
// migration and releases it after, and the savepoint is gone when the transaction was ended, also
// when a new one was begun in its place. It can only detect: what a COMMIT committed stays.
describe("a migration that ends the transaction by another way", () => {
  /** A file and the connection under it, and a module whose migration is given the connection. */
  function viaTheConnection(
    work: (tx: SqlTx, sqlite: { exec(sql: string): void }) => Promise<void>,
  ) {
    const file = tempDatabase();
    const { db, sqlite } = file.connect();
    const m: SchemaModule = {
      id: "a",
      version: 1,
      migrate: (tx) => work(tx, sqlite),
    };
    return { db, m, close: () => file.close() };
  }
  const ENDED =
    "module a ended the transaction that the manager holds; what it did before that is committed or lost and cannot be taken back";

  test.each<[string, string[]]>([
    ["COMMIT", ["COMMIT"]],
    ["END", ["END"]],
    ["COMMIT TRANSACTION", ["COMMIT TRANSACTION"]],
    ["ROLLBACK", ["ROLLBACK"]],
    ["COMMIT and a new BEGIN, so that a transaction is open", ["COMMIT", "BEGIN"]],
    ["ROLLBACK and a new BEGIN", ["ROLLBACK", "BEGIN"]],
  ])("%s is found, and the module is not recorded", async (_, statements) => {
    const { db, m, close } = viaTheConnection(async (tx, sqlite) => {
      await tx.exec("CREATE TABLE a_items (id TEXT)");
      for (const statement of statements) sqlite.exec(statement);
    });
    try {
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect(error, String(error)).toBeInstanceOf(SchemaError);
      expect((error as Error).message).toBe(ENDED);
      // Nothing was recorded, and the layout was not set. What the COMMIT kept stays: the point of
      // the guard is that a migration is not given the way to do this.
      const tables = (await objectsOf(db)).map((o) => o.name);
      const recorded = tables.includes("module_schema")
        ? await db.all("SELECT * FROM module_schema")
        : [];
      expect(recorded).toEqual([]);
      expect(await layout(db)).toBe(0);
      // And nothing is left open: the file can be used and written to at once.
      await db.exec("CREATE TABLE after_it (id TEXT)");
    } finally {
      close();
    }
  });

  test("after a COMMIT the work before it is kept, and the work after it is not taken back, and that is said", async () => {
    const { db, m, close } = viaTheConnection(async (tx, sqlite) => {
      await tx.exec("CREATE TABLE a_items (id TEXT)");
      sqlite.exec("COMMIT");
      await tx.exec("CREATE TABLE a_more (id TEXT)");
    });
    try {
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(ENDED);
      const tables = (await objectsOf(db)).map((o) => o.name);
      expect(tables).toContain("a_items");
      expect(tables).toContain("a_more");
    } finally {
      close();
    }
  });

  test("a ROLLBACK takes the manager's own table with it, and the error says so all the same", async () => {
    const { db, m, close } = viaTheConnection(async (tx, sqlite) => {
      await tx.exec("CREATE TABLE a_items (id TEXT)");
      sqlite.exec("ROLLBACK");
      await tx.exec("CREATE TABLE a_later (id TEXT)");
    });
    try {
      const error = await ensureModules(db, [m]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(ENDED);
      expect((await objectsOf(db)).map((o) => o.name)).toEqual(["a_later"]);
    } finally {
      close();
    }
  });

  test("a migration that does not do it is not touched by the check", async () => {
    const { db, m, close } = viaTheConnection(async (tx) => {
      await tx.exec("CREATE TABLE a_items (id TEXT)");
    });
    try {
      await ensureModules(db, [m]);
      expect(await db.all("SELECT * FROM module_schema")).toEqual([{ module: "a", version: 1 }]);
      expect(await layout(db)).toBe(1);
    } finally {
      close();
    }
  });

  test("the second module is checked too, and the first one's work is taken back with the failure", async () => {
    const file = tempDatabase();
    const { db, sqlite } = file.connect();
    try {
      const first: SchemaModule = {
        id: "first",
        version: 1,
        migrate: (tx) => tx.exec("CREATE TABLE first_items (id TEXT)"),
      };
      const second: SchemaModule = {
        id: "second",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE second_items (id TEXT)");
          sqlite.exec("COMMIT");
        },
      };
      const error = await ensureModules(db, [first, second]).catch((e: unknown) => e);
      expect((error as Error).message).toBe(
        "module second ended the transaction that the manager holds; what it did before that is committed or lost and cannot be taken back",
      );
    } finally {
      file.close();
    }
  });
});

describe("the versions in module_schema are the manager's alone", () => {
  /** A file where module `a` is at version 1 and `b` at version 1, and a module `a` that moves to 2. */
  async function fileWithAB(): Promise<SqlDb> {
    const db = memoryDb();
    await ensureModules(db, [
      { id: "a", version: 1, migrate: (tx) => tx.exec("CREATE TABLE a_items (id TEXT)") },
      { id: "b", version: 1, migrate: (tx) => tx.exec("CREATE TABLE b_items (id TEXT)") },
    ]);
    return db;
  }
  const aAt2 = (statements: string[]): SchemaModule => ({
    id: "a",
    version: 2,
    async migrate(tx, from) {
      if (from < 2) for (const statement of statements) await tx.exec(statement);
    },
  });
  const BRIEF = "only the manager writes the versions";
  const recorded = (db: SqlDb) =>
    db.all<{ module: string; version: number }>(
      "SELECT module, version FROM module_schema ORDER BY module",
    );

  test.each<[string, string[], string]>([
    [
      "its own version moved on",
      ["UPDATE module_schema SET version = 9 WHERE module = 'a'"],
      `module a changed the version recorded for a in module_schema (1 before, 9 after); ${BRIEF}`,
    ],
    [
      "the version of another module moved",
      ["UPDATE module_schema SET version = 9 WHERE module = 'b'"],
      `module a changed the version recorded for b in module_schema (1 before, 9 after); ${BRIEF}`,
    ],
    [
      "every version set",
      ["UPDATE module_schema SET version = version + 1"],
      `module a changed the version recorded for a in module_schema (1 before, 2 after); ${BRIEF}`,
    ],
    [
      "its own row deleted",
      ["DELETE FROM module_schema WHERE module = 'a'"],
      "module a lost rows of the table module_schema (2 before, 1 after); only the manager writes the versions",
    ],
    [
      "another module's row deleted",
      ["DELETE FROM module_schema WHERE module = 'b'"],
      "module a lost rows of the table module_schema (2 before, 1 after); only the manager writes the versions",
    ],
    [
      "all the rows deleted",
      ["DELETE FROM module_schema"],
      "module a lost rows of the table module_schema (2 before, 0 after); only the manager writes the versions",
    ],
    [
      "a row put in for a module that is not there",
      ["INSERT INTO module_schema (module, version) VALUES ('zz', 1)"],
      `module a recorded a version (1) for zz in module_schema; ${BRIEF}`,
    ],
    [
      "a row replaced by another one, so that the number of rows is the same",
      [
        "DELETE FROM module_schema WHERE module = 'b'",
        "INSERT INTO module_schema (module, version) VALUES ('zz', 1)",
      ],
      `module a deleted the version recorded for b in module_schema (1 before); ${BRIEF}`,
    ],
    [
      "a row deleted and put in again with another version",
      [
        "DELETE FROM module_schema WHERE module = 'b'",
        "INSERT INTO module_schema (module, version) VALUES ('b', 7)",
      ],
      `module a changed the version recorded for b in module_schema (1 before, 7 after); ${BRIEF}`,
    ],
  ])("%s is refused and taken back", async (_, statements, message) => {
    const db = await fileWithAB();
    const before = await recorded(db);
    const error = await ensureModules(db, [
      aAt2(statements),
      {
        id: "b",
        version: 1,
        migrate: async () => undefined,
      },
    ]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toBe(message);
    expect(await recorded(db)).toEqual(before);
    expect(before).toEqual([
      { module: "a", version: 1 },
      { module: "b", version: 1 },
    ]);
  });

  // The words of the message for lost rows depend on the table: the rows of module_schema are the
  // manager's, and the rows of any other table may be added to and not deleted.
  test("the message for lost rows says what is true of the table", async () => {
    const lost = async (statement: string) => {
      const db = await legacyFile();
      const error = await ensureModules(db, [
        HOST_SCHEMA,
        {
          ...MILEAGE_SCHEMA,
          version: 2,
          async migrate(tx, from) {
            await MILEAGE_SCHEMA.migrate(tx, from);
            if (from < 2) await tx.exec(statement);
          },
        },
      ]).catch((e: unknown) => e);
      return (error as Error).message;
    };
    expect(await lost("DELETE FROM point")).toBe(
      "module mileage lost rows of the table point (3 before, 0 after); a migration may add rows and may not delete any",
    );
    expect(await lost("DELETE FROM module_schema WHERE module = 'host'")).toBe(
      "module mileage lost rows of the table module_schema (2 before, 1 after); only the manager writes the versions",
    );
  });

  test("a version moved up cannot hide a step: the next start does not refuse the file either way", async () => {
    const db = await fileWithAB();
    await ensureModules(db, [
      aAt2(["UPDATE module_schema SET version = 9 WHERE module = 'a'"]),
      {
        id: "b",
        version: 1,
        migrate: async () => undefined,
      },
    ]).catch(() => undefined);
    // Refused and taken back, so the file does not say 9 and the next start opens it.
    await ensureModules(db, [
      { id: "a", version: 1, migrate: async () => undefined },
      { id: "b", version: 1, migrate: async () => undefined },
    ]);
    expect(await recorded(db)).toEqual([
      { module: "a", version: 1 },
      { module: "b", version: 1 },
    ]);
  });

  test("the manager's own writes are not refused: a module moves up, a new module is recorded", async () => {
    const db = await fileWithAB();
    await ensureModules(db, [
      aAt2(["CREATE TABLE a_more (id TEXT)"]),
      { id: "b", version: 1, migrate: async () => undefined },
      { id: "c", version: 1, migrate: (tx) => tx.exec("CREATE TABLE c_items (id TEXT)") },
    ]);
    expect(await recorded(db)).toEqual([
      { module: "a", version: 2 },
      { module: "b", version: 1 },
      { module: "c", version: 1 },
    ]);
  });

  test("a file of the first build: the versions the manager adopted are kept, the rows of the host too", async () => {
    const db = await legacyFile();
    const error = await ensureModules(db, [
      HOST_SCHEMA,
      {
        ...MILEAGE_SCHEMA,
        version: 2,
        async migrate(tx, from) {
          await MILEAGE_SCHEMA.migrate(tx, from);
          await tx.exec("UPDATE module_schema SET version = 9 WHERE module = 'host'");
        },
      },
    ]).catch((e: unknown) => e);
    expect((error as Error).message).toBe(
      `module mileage changed the version recorded for host in module_schema (1 before, 9 after); ${BRIEF}`,
    );
    expect(await layout(db)).toBe(1);
    expect(
      (await objectsOf(db)).some((o) => o.name === "module_schema"),
      "the table the manager made is taken back with the rest",
    ).toBe(false);
  });
});

// Two limits of the check that 8.7.1 states, held by a test so that they are limits and not guesses.
describe("what the look at the first layout and the layout version leave open", () => {
  const DEFINITION = "CREATE TABLE IF NOT EXISTS trip (id TEXT PRIMARY KEY NOT NULL, x TEXT)";
  const earlier = (extra: string): SchemaModule => ({
    id: "e",
    version: 1,
    migrate: (tx) => tx.exec(`CREATE TABLE e_note (id TEXT PRIMARY KEY NOT NULL, ${extra})`),
  });
  const later: SchemaModule = {
    id: "l",
    version: 1,
    legacyVersion: 1,
    legacyTables: ["trip"],
    legacyDefinitions: { trip: DEFINITION },
    migrate: (tx) => tx.exec(DEFINITION),
  };

  test("a foreign key from an earlier module's table to a table of a later module's first layout: accepted in a new file, refused in an old one", async () => {
    const extra = "trip TEXT REFERENCES trip(id)";
    // A new file: when the earlier module migrates, the later module's trip is not in the file yet.
    const fresh = memoryDb();
    await ensureModules(fresh, [earlier(extra), later]);
    expect((await objectsOf(fresh)).map((o) => o.name)).toContain("e_note");
    // An old file (layout 1, no module_schema): the later module's table is there.
    const old = memoryDb();
    await old.exec(`${DEFINITION}; PRAGMA user_version = 1`);
    const error = await ensureModules(old, [earlier(extra), later]).catch((e: unknown) => e);
    expect(error, String(error)).toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(
      /^module e made the table e_note with a foreign key to trip, which was in the file before/,
    );
  });

  test("a foreign key to the migrating module's own first layout is refused in both", async () => {
    const own: SchemaModule = {
      ...later,
      id: "own",
      migrate: async (tx) => {
        await tx.exec(DEFINITION);
        await tx.exec(
          "CREATE TABLE own_note (id TEXT PRIMARY KEY NOT NULL, t TEXT REFERENCES trip(id))",
        );
      },
    };
    const fresh = await ensureModules(memoryDb(), [own]).catch((e: unknown) => e);
    expect((fresh as Error).message).toMatch(
      /^module own made the table own_note with a foreign key to trip/,
    );
    const old = memoryDb();
    await old.exec(`${DEFINITION}; PRAGMA user_version = 1`);
    const error = await ensureModules(old, [{ ...own, version: 2 }]).catch((e: unknown) => e);
    expect((error as Error).message).toMatch(
      /^module own made the table own_note with a foreign key to trip/,
    );
  });

  const setting = (value: number): SchemaModule => ({
    id: "a",
    version: 2,
    legacyVersion: 1,
    migrate: (tx) => tx.exec(`PRAGMA user_version = ${value}`),
  });
  const USER_VERSION =
    "module a changed PRAGMA user_version; the layout version is LAYOUT_VERSION, set by the manager";

  test("PRAGMA user_version = 0 passes in a new file and is refused in an old one; = 1 the other way round", async () => {
    // A new file has layout 0 before the migration, an old one has 1.
    await ensureModules(memoryDb(), [setting(0)]);
    const zero = memoryDb();
    await zero.exec("PRAGMA user_version = 1");
    const refusedZero = await ensureModules(zero, [setting(0)]).catch((e: unknown) => e);
    expect((refusedZero as Error).message).toBe(USER_VERSION);

    const one = memoryDb();
    await one.exec("PRAGMA user_version = 1");
    await ensureModules(one, [setting(1)]);
    const refusedOne = await ensureModules(memoryDb(), [setting(1)]).catch((e: unknown) => e);
    expect((refusedOne as Error).message).toBe(USER_VERSION);
  });

  test("any other number is refused in both", async () => {
    for (const value of [2, 7, -1]) {
      const old = memoryDb();
      await old.exec("PRAGMA user_version = 1");
      for (const db of [memoryDb(), old]) {
        const error = await ensureModules(db, [setting(value)]).catch((e: unknown) => e);
        expect((error as Error).message, String(value)).toBe(USER_VERSION);
      }
    }
  });

  test("the file ends with layout 1 whichever of the two passed", async () => {
    const fresh = memoryDb();
    await ensureModules(fresh, [setting(0)]);
    expect(await layout(fresh)).toBe(1);
    const old = await emptyLegacyFile();
    await ensureModules(old, [HOST_SCHEMA, MILEAGE_SCHEMA]);
    expect(await layout(old)).toBe(1);
  });
});

// A migration that is given values to bind gets them: the guarded transaction hands `params` on
// through each of the three methods that take them, and a migration that used a parameter would
// otherwise write NULL or find nothing without any test noticing.
describe("a migration binds parameters through run, all and one", () => {
  test("each method hands its parameters on, and what was bound is in the file afterwards", async () => {
    const db = memoryDb();
    const seen: Record<string, unknown> = {};
    const m: SchemaModule = {
      id: "a",
      version: 1,
      async migrate(tx) {
        await tx.exec("CREATE TABLE a_log (note TEXT, n INTEGER)");
        await tx.run("INSERT INTO a_log (note, n) VALUES (?, ?)", ["kept", 7]);
        await tx.run("INSERT INTO a_log (note, n) VALUES (?, ?)", ["other", 8]);
        seen.all = await tx.all("SELECT note, n FROM a_log WHERE note = ? AND n = ?", ["kept", 7]);
        seen.one = await tx.one("SELECT note, n FROM a_log WHERE note = ? AND n = ?", ["other", 8]);
        seen.none = await tx.one("SELECT note FROM a_log WHERE note = ?", ["nothing like it"]);
      },
    };
    await ensureModules(db, [m]);
    expect(seen.all).toEqual([{ note: "kept", n: 7 }]);
    expect(seen.one).toEqual({ note: "other", n: 8 });
    expect(seen.none).toBeNull();
    expect(await db.all("SELECT note, n FROM a_log ORDER BY n")).toEqual([
      { note: "kept", n: 7 },
      { note: "other", n: 8 },
    ]);
  });

  test("run binds: a parameter is written, not NULL", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      {
        id: "a",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE a_log (note TEXT)");
          await tx.run("INSERT INTO a_log (note) VALUES (?)", ["kept"]);
        },
      },
    ]);
    expect(await db.all("SELECT note FROM a_log")).toEqual([{ note: "kept" }]);
  });

  test("all binds: a parameter selects the row", async () => {
    const db = memoryDb();
    let rows: unknown[] = [];
    await ensureModules(db, [
      {
        id: "a",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE a_log (note TEXT)");
          await tx.exec("INSERT INTO a_log (note) VALUES ('kept'), ('other')");
          rows = await tx.all("SELECT note FROM a_log WHERE note = ?", ["kept"]);
        },
      },
    ]);
    expect(rows).toEqual([{ note: "kept" }]);
  });

  test("one binds: a parameter selects the row", async () => {
    const db = memoryDb();
    let row: unknown = "unset";
    await ensureModules(db, [
      {
        id: "a",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE a_log (note TEXT)");
          await tx.exec("INSERT INTO a_log (note) VALUES ('kept'), ('other')");
          row = await tx.one("SELECT note FROM a_log WHERE note = ?", ["other"]);
        },
      },
    ]);
    expect(row).toEqual({ note: "other" });
  });

  test("a parameter that holds a word the guard refuses is a value, not a statement", async () => {
    const db = memoryDb();
    await ensureModules(db, [
      {
        id: "a",
        version: 1,
        async migrate(tx) {
          await tx.exec("CREATE TABLE a_log (note TEXT)");
          await tx.run("INSERT INTO a_log (note) VALUES (?)", ["COMMIT; ROLLBACK; TEMP"]);
        },
      },
    ]);
    expect(await db.all("SELECT note FROM a_log")).toEqual([{ note: "COMMIT; ROLLBACK; TEMP" }]);
  });
});

// The table is the manager's. Each case is a migration of a module on a file in which the table is
// there before the migration runs (in a new file the manager makes it first), and each is refused
// with the lint's own message and taken back. Lowering a version makes the next start run that
// module's migration again; dropping or altering the table takes away what records the versions.
describe("module_schema cannot be lowered, dropped or altered by a migration", () => {
  const BRIEF = "only the manager writes the versions";
  const starts: [string, () => Promise<SqlDb>][] = [
    ["a new file", async () => memoryDb()],
    ["a file of the first build with no rows", emptyLegacyFile],
    ["a file of the first build with rows", legacyFile],
  ];
  const mileageDoing = (statements: string[]): SchemaModule => ({
    ...MILEAGE_SCHEMA,
    version: 2,
    async migrate(tx, from) {
      await MILEAGE_SCHEMA.migrate(tx, from);
      if (from < 2) for (const statement of statements) await tx.exec(statement);
    },
  });
  const CASES: [string, string[], string][] = [
    [
      "the version of the module before it lowered to 0",
      ["UPDATE module_schema SET version = 0 WHERE module = 'host'"],
      `module mileage changed the version recorded for host in module_schema (1 before, 0 after); ${BRIEF}`,
    ],
    [
      "the version of the module before it lowered by one",
      ["UPDATE module_schema SET version = version - 1 WHERE module = 'host'"],
      `module mileage changed the version recorded for host in module_schema (1 before, 0 after); ${BRIEF}`,
    ],
    [
      "the table dropped",
      ["DROP TABLE module_schema"],
      "module mileage changed the table module_schema, which belongs to another module",
    ],
    [
      "the table dropped and made again with another column",
      [
        "DROP TABLE module_schema",
        "CREATE TABLE module_schema (module TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL, x TEXT) WITHOUT ROWID",
      ],
      "module mileage changed the table module_schema, which belongs to another module",
    ],
    [
      "a column added",
      ["ALTER TABLE module_schema ADD COLUMN x TEXT"],
      "module mileage changed the table module_schema, which belongs to another module",
    ],
    [
      "the table renamed",
      ["ALTER TABLE module_schema RENAME TO mileage_schema"],
      "module mileage changed the table module_schema, which belongs to another module",
    ],
    [
      "an index made on it",
      ["CREATE INDEX mileage_idx ON module_schema (version)"],
      "module mileage made the index mileage_idx on module_schema, which it does not own (its tables are named mileage_*)",
    ],
  ];

  describe.each(starts)("in %s", (_, start) => {
    test.each(CASES)(
      "%s is refused with the lint's message and taken back",
      async (__, list, message) => {
        const db = await start();
        const before = await objectsOf(db);
        const layoutBefore = await layout(db);
        const error = await ensureModules(db, [HOST_SCHEMA, mileageDoing(list)]).catch(
          (e: unknown) => e,
        );
        expect(error, String(error)).toBeInstanceOf(SchemaError);
        expect((error as Error).message).toBe(message);
        expect(await objectsOf(db)).toEqual(before);
        expect(await layout(db)).toBe(layoutBefore);
      },
    );
  });

  // The manager's rows are compared before and after, not watched: a write with no net effect is
  // not a change, and passes. (Writes that change a version or the set of rows are refused above
  // and in the tests of the versions.)
  describe("a write that changes nothing is not a change", () => {
    test.each([
      ["a version set to itself", ["UPDATE module_schema SET version = version"]],
      [
        "a row replaced by the same row",
        ["INSERT OR REPLACE INTO module_schema (module, version) VALUES ('host', 1)"],
      ],
      [
        "REPLACE of the same row",
        ["REPLACE INTO module_schema (module, version) VALUES ('host', 1)"],
      ],
      [
        "an insert that does nothing on a conflict",
        [
          "INSERT INTO module_schema (module, version) VALUES ('host', 1) ON CONFLICT(module) DO NOTHING",
        ],
      ],
      [
        "a row deleted and put in again as it was",
        [
          "DELETE FROM module_schema WHERE module = 'host'",
          "INSERT INTO module_schema (module, version) VALUES ('host', 1)",
        ],
      ],
    ])("%s passes", async (_, list) => {
      const db = await legacyFile();
      await ensureModules(db, [HOST_SCHEMA, mileageDoing(list)]);
      expect(await db.all("SELECT * FROM module_schema ORDER BY module")).toEqual([
        { module: "host", version: 1 },
        { module: "mileage", version: 2 },
      ]);
    });

    test("the table dropped and made again in its own text, with its rows put back, passes", async () => {
      const db = await legacyFile();
      await ensureModules(db, [
        HOST_SCHEMA,
        {
          ...MILEAGE_SCHEMA,
          version: 2,
          async migrate(tx, from) {
            await MILEAGE_SCHEMA.migrate(tx, from);
            if (from >= 2) return;
            const text = (await tx.one<{ sql: string }>(
              "SELECT sql FROM sqlite_master WHERE name = 'module_schema'",
            ))!.sql;
            const rows = await tx.all<{ module: string; version: number }>(
              "SELECT module, version FROM module_schema",
            );
            await tx.exec("DROP TABLE module_schema");
            await tx.exec(text);
            for (const row of rows) {
              await tx.run("INSERT INTO module_schema (module, version) VALUES (?, ?)", [
                row.module,
                row.version,
              ]);
            }
          },
        },
      ]);
      expect(await db.all("SELECT * FROM module_schema ORDER BY module")).toEqual([
        { module: "host", version: 1 },
        { module: "mileage", version: 2 },
      ]);
    });
  });
});

// A limit that 8.7.1 states: the lint reads text and does not run the statements on the rows of a
// phone. A migration whose own statements succeed or fail depending on the rows passes on a new
// file and on a file with no trips, and fails on a phone that has trips. The failure takes the
// migration back and is reported (with the database's own error), so nothing is damaged, but it is
// found on the phone and not by the lint. The one rule of this kind that the lint can see in the
// text (a default of the time of the insert on an added column) is refused by its words.
describe("a limit: a migration whose own statements depend on the rows", () => {
  const dependsOnTrips: SchemaModule = {
    ...MILEAGE_SCHEMA,
    version: 2,
    async migrate(tx, from) {
      await MILEAGE_SCHEMA.migrate(tx, from);
      if (from >= 2) return;
      await tx.exec("CREATE TABLE mileage_x (n INTEGER CHECK (n > 0))");
      await tx.exec("INSERT INTO mileage_x (n) SELECT 0 FROM trip");
    },
  };

  test.each([
    ["a new file", async () => memoryDb()],
    ["a file of the first build with no trips", emptyLegacyFile],
  ] as [string, () => Promise<SqlDb>][])("it passes on %s", async (_, start) => {
    const db = await start();
    await ensureModules(db, [HOST_SCHEMA, dependsOnTrips]);
    expect(await db.all("SELECT * FROM mileage_x")).toEqual([]);
  });

  test("it fails on a file with trips, with the database's own error, and the file is as it was", async () => {
    const db = await legacyFile();
    const objects = await objectsOf(db);
    const rows = await rowsOf(db);
    const error = await ensureModules(db, [HOST_SCHEMA, dependsOnTrips]).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(Error);
    expect(error).not.toBeInstanceOf(SchemaError);
    expect((error as Error).message).toMatch(/CHECK constraint failed/);
    expect(await objectsOf(db)).toEqual(objects);
    expect(await rowsOf(db)).toEqual(rows);
    expect(await layout(db)).toBe(1);
  });
});

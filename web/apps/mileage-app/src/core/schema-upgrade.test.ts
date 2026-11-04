import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA, createHostState } from "./host-state.ts";
import { openOutbox, policyFor, type OpenNames } from "./open-outbox.ts";
import { ensureModules, type SchemaModule } from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { MILEAGE_SCHEMA, createSqlStore, migrate } from "./sql-store.ts";
import { FakeDatabaseFile, FakeSecrets, fakePorts } from "../test-support/fake-file.ts";
import {
  SCHEMA as LEGACY_SCHEMA,
  migrate as legacyMigrate,
} from "../test-support/legacy-schema.ts";
import { memoryDb } from "../test-support/memory-db.ts";
import { watched } from "../test-support/watched-db.ts";

// The real modules (the host's and the outbox's) against the files that the first builds made.
// An installed app has such a file, and the update must open it as it is: no row moved, no table
// rewritten, nothing written that an earlier build would not understand. The other direction is
// tested too: an earlier build opening a file that this build has touched.
//
// "The first build" is `test-support/legacy-schema.ts`, a frozen copy of the schema and the
// migration as they were before the schema manager. All data here is made up.

const KEY = "a".repeat(64);
const NAMES: OpenNames = { databaseKey: "db-key", session: ["server", "tokens"] };
const MODULES = [HOST_SCHEMA, MILEAGE_SCHEMA];
const T0 = Date.parse("2026-10-15T09:00:00Z");

/** The tables of the first layout, and no other. */
const LEGACY_TABLES = ["batch", "meta", "point", "trip"];

/** A file as the first build leaves it after a few days of use. */
async function seed(db: SqlDb): Promise<void> {
  const trip = (id: string, at: number) =>
    db.run(
      "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES (?, ?, ?, ?)",
      [id, "business", new Date(at).toISOString(), at],
    );
  await trip("c1", T0);
  await trip("c2", T0 + 3_600_000);
  for (const seq of [1, 2, 3]) {
    await db.run("INSERT INTO point (client_trip_id, seq, t, lat, lon) VALUES (?, ?, ?, ?, ?)", [
      "c1",
      seq,
      T0 + seq * 1000,
      `60.17${seq}`,
      `24.94${seq}`,
    ]);
  }
  await db.run(
    "INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES (?, ?, ?, ?)",
    ["c1", 1, 3, 1],
  );
  const meta = (key: string, value: string) =>
    db.run("INSERT INTO meta (key, value) VALUES (?, ?)", [key, value]);
  await meta("owner", JSON.stringify({ origin: "https://phone.example", user: "dave" }));
  await meta("location_disclosure_accepted", "2026-10-01T08:00:00.000Z");
  await meta("stale_file_recovered_at", "2026-10-30T10:00:00.000Z");
  await meta("active_trip", "c2");
}

async function legacyFile(): Promise<SqlDb> {
  const db = memoryDb();
  await legacyMigrate(db);
  await seed(db);
  return db;
}

interface Entry {
  type: string;
  name: string;
  tbl_name: string;
  sql: string | null;
}

/** The definitions in the file, as SQLite keeps them, by name. */
const objects = (db: SqlDb, names?: readonly string[]) =>
  db
    .all<Entry>(
      "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY type, name",
    )
    .then((rows) => (names === undefined ? rows : rows.filter((r) => names.includes(r.tbl_name))));

/** Every row of the four tables of the first layout. */
async function rows(db: SqlDb) {
  const out: Record<string, unknown[]> = {};
  for (const table of LEGACY_TABLES) {
    const order = table === "meta" ? "key" : table === "trip" ? "client_trip_id" : "1, 2";
    out[table] = await db.all(`SELECT * FROM ${table} ORDER BY ${order}`);
  }
  return out;
}

const layout = async (db: SqlDb) =>
  Number((await db.one<{ user_version: number }>("PRAGMA user_version"))?.user_version);
const tableNames = async (db: SqlDb) =>
  (await objects(db)).filter((o) => o.type === "table").map((o) => o.name);
const recorded = async (db: SqlDb) =>
  Object.fromEntries(
    (
      await db.all<{ module: string; version: number }>("SELECT module, version FROM module_schema")
    ).map((r) => [r.module, r.version]),
  );

/** A later version of the outbox's module, the way a build that moves it on would write it. */
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

describe("the frozen copy of the first build", () => {
  test("is the text of the commit before the schema manager", () => {
    // Computed from `sql-store.ts` of that commit when this was written, not from the copy.
    expect(createHash("sha256").update(LEGACY_SCHEMA).digest("hex")).toBe(
      "bfb2567f01c6af591e7e868e75822f3d0bda079caa018b2fa9cd9da928c72982",
    );
    const source = readFileSync(
      new URL("../test-support/legacy-schema.ts", import.meta.url),
      "utf8",
    );
    const body = source.slice(source.indexOf("/** Bump with a migration step"));
    expect(createHash("sha256").update(body).digest("hex")).toBe(
      "369d8551717ecb536a7c4ebee3694f379a03a5cba5d0f7d618c2bbda76f87b95",
    );
  });

  test("makes the four tables and sets the layout to 1", async () => {
    const db = memoryDb();
    await legacyMigrate(db);
    expect(await tableNames(db)).toEqual(LEGACY_TABLES);
    expect(await layout(db)).toBe(1);
  });
});

describe("a file that the first build made, opened by this build", () => {
  test("is read and not written: no statement, no transaction, and nothing in it has changed", async () => {
    const db = await legacyFile();
    const before = { objects: await objects(db), rows: await rows(db) };
    const { spy, log } = watched(db);

    await migrate(spy);
    await ensureModules(spy, MODULES);

    expect(log).toEqual([]);
    expect(await objects(db)).toEqual(before.objects);
    expect(await rows(db)).toEqual(before.rows);
    expect(await layout(db)).toBe(1);
    expect(await tableNames(db)).toEqual(LEGACY_TABLES);
  });

  test("is used by the store and by the host's way in as it was", async () => {
    const db = await legacyFile();
    await migrate(db);
    const store = createSqlStore(db);
    expect((await store.listTrips()).map((t) => t.clientTripId).sort()).toEqual(["c1", "c2"]);
    expect((await store.activeTrip())?.clientTripId).toBe("c2");
    expect(await store.pointCount("c1")).toBe(3);
    const host = createHostState(db);
    expect(await host.getMeta("owner")).toBe(
      JSON.stringify({ origin: "https://phone.example", user: "dave" }),
    );
    expect(await host.getMeta("location_disclosure_accepted")).toBe("2026-10-01T08:00:00.000Z");
    expect(await host.getMeta("stale_file_recovered_at")).toBe("2026-10-30T10:00:00.000Z");
  });

  test("is adopted when a module is added later: the old tables and rows are untouched and the old modules are recorded at version 1", async () => {
    const db = await legacyFile();
    const before = { objects: await objects(db, LEGACY_TABLES), rows: await rows(db) };
    const extra: SchemaModule = {
      id: "extra",
      version: 1,
      requires: ["host"],
      migrate: (tx) => tx.exec("CREATE TABLE extra_items (id TEXT PRIMARY KEY NOT NULL)"),
    };

    await ensureModules(db, [...MODULES, extra]);

    expect(await recorded(db)).toEqual({ host: 1, mileage: 1, extra: 1 });
    expect(await objects(db, LEGACY_TABLES)).toEqual(before.objects);
    expect(await rows(db)).toEqual(before.rows);
    expect(await layout(db)).toBe(1);
    expect(await tableNames(db)).toEqual([
      "batch",
      "extra_items",
      "meta",
      "module_schema",
      "point",
      "trip",
    ]);
  });

  test("is adopted when the outbox's module moves on: it starts from version 1, not from nothing", async () => {
    const db = await legacyFile();
    const before = await rows(db);
    const v2 = mileageAt2(["ALTER TABLE trip ADD COLUMN note TEXT"]);
    await ensureModules(db, [HOST_SCHEMA, v2]);
    expect(await recorded(db)).toEqual({ host: 1, mileage: 2 });
    // Every row is there, with the new column empty.
    const trips = await db.all<{ client_trip_id: string; note: string | null }>(
      "SELECT client_trip_id, note FROM trip ORDER BY client_trip_id",
    );
    expect(trips).toEqual([
      { client_trip_id: "c1", note: null },
      { client_trip_id: "c2", note: null },
    ]);
    expect((await rows(db)).meta).toEqual(before.meta);
    expect((await rows(db)).point).toEqual(before.point);
  });
});

describe("a file that this build made, opened by the first build", () => {
  test("has the same four tables, with the same definitions, as a file the first build made", async () => {
    const first = memoryDb();
    await legacyMigrate(first);
    const now = memoryDb();
    await migrate(now);
    // The statements are compared as SQLite keeps them, table by table.
    expect(await objects(now, LEGACY_TABLES)).toEqual(await objects(first, LEGACY_TABLES));
    expect(await layout(now)).toBe(1);
    // The one addition: the versions of the modules, which the first build does not look at.
    expect(await tableNames(now)).toEqual([...LEGACY_TABLES, "module_schema"].sort());
    expect(await recorded(now)).toEqual({ host: 1, mileage: 1 });
  });

  test("is opened by the first build's migration without a write, and the first build's statements work on it", async () => {
    const db = memoryDb();
    await migrate(db);
    const { spy, log } = watched(db);
    await legacyMigrate(spy);
    expect(log).toEqual([]);

    // The statements of the store are the first build's (this change did not edit them).
    const store = createSqlStore(db);
    const trip = await store.startTrip({
      clientTripId: "n1",
      purpose: "business",
      startedAt: new Date(T0).toISOString(),
      nowMs: T0,
    });
    expect(trip.clientTripId).toBe("n1");
    expect((await store.activeTrip())?.clientTripId).toBe("n1");
    await store.stopTrip("n1", new Date(T0 + 1000).toISOString());
    await store.clearAll();
    expect(await store.listTrips()).toEqual([]);
    // The first build leaves the version table alone.
    expect(await recorded(db)).toEqual({ host: 1, mileage: 1 });
  });

  describe("after a module has moved on by additions only", () => {
    const additions = [
      "ALTER TABLE trip ADD COLUMN note TEXT",
      "ALTER TABLE trip ADD COLUMN flag INTEGER NOT NULL DEFAULT 0",
      "CREATE TABLE mileage_stop (id TEXT PRIMARY KEY NOT NULL, trip TEXT)",
    ];

    async function movedOn() {
      const db = await legacyFile();
      await ensureModules(db, [HOST_SCHEMA, mileageAt2(additions)]);
      return db;
    }

    test("the first build opens it: it does not read the version table, and the layout is still 1", async () => {
      const db = await movedOn();
      expect(await recorded(db)).toEqual({ host: 1, mileage: 2 });
      expect(await layout(db)).toBe(1);
      const { spy, log } = watched(db);
      await legacyMigrate(spy);
      expect(log).toEqual([]);
    });

    test("and the statements of the first build work: a trip inserted with the old columns gets the defaults", async () => {
      const db = await movedOn();
      const store = createSqlStore(db);
      await store.stopTrip("c2", new Date(T0 + 7_200_000).toISOString());
      await store.startTrip({
        clientTripId: "old1",
        purpose: "private",
        startedAt: new Date(T0).toISOString(),
        nowMs: T0,
      });
      const row = await db.one<{ note: string | null; flag: number }>(
        "SELECT note, flag FROM trip WHERE client_trip_id = 'old1'",
      );
      expect(row).toEqual({ note: null, flag: 0 });
      // The rest of what the store does with a trip.
      await store.stopTrip("old1", new Date(T0 + 1000).toISOString());
      expect((await store.getTrip("old1"))?.stopRequested).toBe(true);
      await store.deleteTrip("old1");
      expect(await store.getTrip("old1")).toBeNull();
    });

    test("a build that knows version 1 of the module only is refused the file, and writes nothing", async () => {
      const db = await movedOn();
      const before = await objects(db);
      const { spy, log } = watched(db);
      await expect(ensureModules(spy, MODULES)).rejects.toThrow(
        /newer version of the app \(schema 1, mileage 2\)/,
      );
      expect(log).toEqual([]);
      expect(await objects(db)).toEqual(before);
    });
  });

  test("a change that the first build could not read is refused when it is made, and the file stays as it was", async () => {
    const db = await legacyFile();
    const before = { objects: await objects(db), rows: await rows(db) };
    const renaming = mileageAt2(["ALTER TABLE trip RENAME COLUMN purpose TO reason"]);
    await expect(ensureModules(db, [HOST_SCHEMA, renaming])).rejects.toThrow(
      /module mileage changed the column trip\.purpose/,
    );
    expect(await objects(db)).toEqual(before.objects);
    expect(await rows(db)).toEqual(before.rows);
    expect(await tableNames(db)).toEqual(LEGACY_TABLES);
    // And the first build still opens it.
    await legacyMigrate(db);
  });
});

describe("opening the encrypted file of an installed app", () => {
  /** An iOS phone with a signed-in session and a file of the first build. */
  async function installed() {
    const file = new FakeDatabaseFile();
    await file.makeFile(KEY, legacyMigrate);
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", KEY);
    secrets.items.set("server", '{"baseUrl":"https://phone.example","clientId":"c"}');
    secrets.items.set("tokens", '{"accessToken":"a","refreshToken":"r","expiresAt":1}');
    const sqlite = file.sqlite!;
    sqlite.exec(
      "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES ('c1', 'business', '2026-10-15T09:00:00.000Z', 1)",
    );
    sqlite.exec(
      'INSERT INTO meta (key, value) VALUES (\'owner\', \'{"origin":"https://phone.example","user":"dave"}\')',
    );
    // The making of the file opened and closed it once; what the tests count is what comes after.
    const made = { opens: file.opens, closes: file.closes, closeCalls: file.closeCalls };
    return { file, secrets, made };
  }

  const names = (file: FakeDatabaseFile) =>
    (
      file
        .sqlite!.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .all() as {
        name: string;
      }[]
    ).map((r) => r.name);

  test("is not a new database: the session is kept, nothing is deleted, nothing is written to the file", async () => {
    const { file, secrets } = await installed();
    const before = names(file);
    const opened = await openOutbox(fakePorts(file, secrets), NAMES, policyFor("ios"), MODULES);
    expect(opened).toMatchObject({ fresh: false, recoveredStaleFile: false });
    expect(secrets.log).toEqual([]);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(secrets.items.has("server")).toBe(true);
    expect(secrets.items.has("tokens")).toBe(true);
    expect(file.deletes).toBe(0);
    expect(names(file)).toEqual(before);
    expect(names(file)).not.toContain("module_schema");
    expect(await createSqlStore(opened.db).getTrip("c1")).not.toBeNull();
  });

  test.each([
    ["a key that was proven by an open", KEY],
    ["a key that is still pending", `pending:${KEY}`],
  ])(
    "a failure while the version table is made rolls everything back, closes the file once and deletes nothing (%s)",
    async (_, stored) => {
      const { file, secrets, made } = await installed();
      secrets.items.set("db-key", stored);
      const extra: SchemaModule = {
        id: "extra",
        version: 1,
        requires: ["host"],
        migrate: (tx) => tx.exec("CREATE TABLE extra_items (id TEXT)"),
      };
      const ports = fakePorts(file, secrets);
      const openFile = ports.openFile.bind(ports);
      ports.openFile = async () => {
        const handle = await openFile();
        return {
          ...handle,
          exec: async (sql) => {
            if (sql.includes("module_schema (module TEXT")) throw new Error("The disk is full");
            return handle.exec(sql);
          },
        };
      };
      const before = names(file);
      await expect(openOutbox(ports, NAMES, policyFor("ios"), [...MODULES, extra])).rejects.toThrow(
        "The disk is full",
      );
      // A failure of the schema is not taken for a file that no key fits.
      expect(file.deletes).toBe(0);
      expect(file.opens - made.opens).toBe(1);
      expect(file.closes - made.closes).toBe(1);
      expect(file.closeCalls - made.closeCalls).toBe(1);
      expect(names(file)).toEqual(before);
      expect(secrets.log).toEqual([]);
      expect(secrets.items.get("db-key")).toBe(stored);
      expect(secrets.items.has("server")).toBe(true);
    },
  );

  test.each([
    ["a key that was proven by an open", KEY],
    ["a key that is still pending", `pending:${KEY}`],
  ])(
    "a file of a newer layout is refused, closed once and not replaced (%s)",
    async (_, stored) => {
      const { file, secrets, made } = await installed();
      secrets.items.set("db-key", stored);
      file.sqlite!.exec("PRAGMA user_version = 2");
      await expect(
        openOutbox(fakePorts(file, secrets), NAMES, policyFor("ios"), MODULES),
      ).rejects.toThrow(/newer version of the app \(schema 2\)/);
      expect(file.deletes).toBe(0);
      expect(file.closes - made.closes).toBe(1);
      expect(file.closeCalls - made.closeCalls).toBe(1);
      expect(secrets.items.has("server")).toBe(true);
      expect(secrets.items.has("tokens")).toBe(true);
    },
  );

  test("a new file gets the modules, the layout and the versions, and the sign-in of the earlier file is dropped as before", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    secrets.items.set("server", "x");
    secrets.items.set("tokens", "y");
    const opened = await openOutbox(
      fakePorts(file, secrets, { key: KEY }),
      NAMES,
      policyFor("ios"),
      MODULES,
    );
    expect(opened.fresh).toBe(true);
    expect(secrets.items.has("server")).toBe(false);
    expect(secrets.items.has("tokens")).toBe(false);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(names(file)).toEqual([...LEGACY_TABLES, "module_schema"].sort());
    expect(await recorded(opened.db)).toEqual({ host: 1, mileage: 1 });
    expect(await layout(opened.db)).toBe(1);
  });
});

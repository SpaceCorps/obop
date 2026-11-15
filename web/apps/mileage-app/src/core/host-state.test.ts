import { describe, expect, test } from "vite-plus/test";
import { bindOwner, bindingOf, parseOwner, takeOver } from "./identity.ts";
import {
  DISCLOSURE_META,
  HOST_SCHEMA,
  META_OWNER,
  META_STALE_FILE_RECOVERED,
  createHostState,
} from "./host-state.ts";
import { SchemaError, ensureModules } from "./schema.ts";
import { MILEAGE_SCHEMA, createSqlStore, migrate } from "./sql-store.ts";
import { memoryDb } from "../test-support/memory-db.ts";

// The rows that belong to the host and not to an app: whose trips the file holds, whether the
// disclosure was read, and the note about a replaced file. They are rows of the table `meta`, where
// the first build put them, and the host reaches them through a way in of its own.

describe("the names of the host's rows", () => {
  // Literals on purpose: they are what a file written by an earlier build holds.
  test("are the ones an installed app has", () => {
    expect(META_OWNER).toBe("owner");
    expect(DISCLOSURE_META).toBe("location_disclosure_accepted");
    expect(META_STALE_FILE_RECOVERED).toBe("stale_file_recovered_at");
    expect([...(HOST_SCHEMA.legacyMetaKeys ?? [])].sort()).toEqual([
      "location_disclosure_accepted",
      "owner",
      "stale_file_recovered_at",
    ]);
    expect(HOST_SCHEMA.legacyTables).toEqual(["meta"]);
    expect(HOST_SCHEMA.id).toBe("host");
  });

  test("do not include the pointer to the trip that is recording, which is the outbox's", () => {
    expect(HOST_SCHEMA.legacyMetaKeys).not.toContain("active_trip");
    expect(MILEAGE_SCHEMA.legacyMetaKeys).toEqual(["active_trip"]);
  });
});

describe("the host's table", () => {
  test("a host module alone makes the table meta with its first definition, and records its version", async () => {
    const db = memoryDb();
    await ensureModules(db, [HOST_SCHEMA]);
    const meta = await db.one<{ sql: string }>(
      "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
    );
    expect(meta?.sql).toBe(
      "CREATE TABLE meta (\n  key TEXT PRIMARY KEY NOT NULL,\n  value TEXT NOT NULL\n) WITHOUT ROWID",
    );
    expect(await db.all("SELECT module, version FROM module_schema")).toEqual([
      { module: "host", version: 1 },
    ]);
  });

  test("keeps a value, replaces it, and removes it with null", async () => {
    const db = memoryDb();
    await migrate(db);
    const host = createHostState(db);
    expect(await host.getMeta(META_OWNER)).toBeNull();
    await host.setMeta(META_OWNER, "one");
    await host.setMeta(META_OWNER, "two");
    expect(await host.getMeta(META_OWNER)).toBe("two");
    expect(await db.all("SELECT key, value FROM meta")).toEqual([{ key: "owner", value: "two" }]);
    await host.setMeta(META_OWNER, null);
    expect(await host.getMeta(META_OWNER)).toBeNull();
    expect(await db.all("SELECT key FROM meta")).toEqual([]);
  });

  test("works on the rows that the outbox's store wrote, and the store on what it wrote: one place, not two", async () => {
    const db = memoryDb();
    await migrate(db);
    const host = createHostState(db);
    const store = createSqlStore(db);
    await store.setMeta(META_OWNER, "from the store");
    expect(await host.getMeta(META_OWNER)).toBe("from the store");
    await host.setMeta(DISCLOSURE_META, "2026-10-01T08:00:00.000Z");
    expect(await store.getMeta(DISCLOSURE_META)).toBe("2026-10-01T08:00:00.000Z");
    await host.setMeta(META_STALE_FILE_RECOVERED, "2026-10-30T10:00:00.000Z");
    expect(
      (await db.all<{ key: string }>("SELECT key FROM meta ORDER BY key")).map((r) => r.key),
    ).toEqual(["location_disclosure_accepted", "owner", "stale_file_recovered_at"]);
  });

  test("writes the same row as the store does for the same key", async () => {
    const viaHost = memoryDb();
    const viaStore = memoryDb();
    await migrate(viaHost);
    await migrate(viaStore);
    await createHostState(viaHost).setMeta(META_OWNER, '{"origin":"a","user":"b"}');
    await createSqlStore(viaStore).setMeta(META_OWNER, '{"origin":"a","user":"b"}');
    const rows = (db: typeof viaHost) => db.all("SELECT key, value FROM meta");
    expect(await rows(viaHost)).toEqual(await rows(viaStore));
  });

  test("does not touch a key that belongs to the outbox or to anything else", async () => {
    const db = memoryDb();
    await migrate(db);
    const host = createHostState(db);
    await createSqlStore(db).setMeta("active_trip", "t1");
    expect(() => host.getMeta("active_trip")).toThrow(SchemaError);
    expect(() => host.setMeta("active_trip", null)).toThrow(/host may use the meta keys host\.\*/);
    expect(() => host.setMeta("mileage.theme", "dark")).toThrow(SchemaError);
    expect(await createSqlStore(db).getMeta("active_trip")).toBe("t1");
  });

  test("a write that fails leaves the row as it was and tells the caller", async () => {
    const db = memoryDb();
    await migrate(db);
    const host = createHostState(db);
    await host.setMeta(META_OWNER, "kept");
    const failing = createHostState({
      ...db,
      transaction: (fn) =>
        db.transaction(async (tx) => {
          await fn(tx);
          throw new Error("The disk is full");
        }),
    });
    await expect(failing.setMeta(META_OWNER, "lost")).rejects.toThrow("The disk is full");
    expect(await host.getMeta(META_OWNER)).toBe("kept");
  });
});

describe("the owner of the outbox decided through the host's state", () => {
  const DAVE = { origin: "https://phone.example", user: "dave" };
  const RITA = { origin: "https://phone.example", user: "rita" };
  const T0 = Date.parse("2026-10-15T09:00:00Z");

  async function rig() {
    const db = memoryDb();
    await migrate(db);
    return { store: createSqlStore(db), host: createHostState(db), db };
  }

  async function unsentTrip(store: ReturnType<typeof createSqlStore>) {
    await store.startTrip({
      clientTripId: "c1",
      purpose: "business",
      startedAt: new Date(T0).toISOString(),
      nowMs: T0,
    });
    await store.stopTrip("c1", new Date(T0 + 1000).toISOString());
  }

  test("a person adopts an outbox that has no owner, and the row is the one the store reads", async () => {
    const { store, host } = await rig();
    await expect(bindOwner(store, DAVE, host)).resolves.toEqual({ kind: "own" });
    expect(parseOwner(await host.getMeta(META_OWNER))).toEqual(DAVE);
    expect(parseOwner(await store.getMeta(META_OWNER))).toEqual(DAVE);
  });

  test("another person finds trips that are unsent as foreign, from a row that the store wrote", async () => {
    const { store, host } = await rig();
    await unsentTrip(store);
    await store.setMeta(META_OWNER, JSON.stringify(DAVE));
    const config = { baseUrl: RITA.origin, clientId: "c", user: RITA.user };
    await expect(bindingOf(store, config, host)).resolves.toMatchObject({ kind: "foreign" });
    await expect(bindOwner(store, RITA, host)).resolves.toMatchObject({ kind: "foreign" });
    expect(parseOwner(await host.getMeta(META_OWNER))).toEqual(DAVE);
  });

  test("taking over clears the outbox's trips and replaces the owner, and keeps the other host rows", async () => {
    const { store, host } = await rig();
    await unsentTrip(store);
    await host.setMeta(META_OWNER, JSON.stringify(DAVE));
    await host.setMeta(DISCLOSURE_META, "2026-10-01T08:00:00.000Z");
    await takeOver(store, RITA, host);
    expect(await store.listTrips()).toEqual([]);
    expect(parseOwner(await host.getMeta(META_OWNER))).toEqual(RITA);
    expect(await host.getMeta(DISCLOSURE_META)).toBe("2026-10-01T08:00:00.000Z");
  });
});

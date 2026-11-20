import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import {
  MIGRATIONS,
  SCHEMA_VERSION,
  SchemaError,
  migrate,
  schemaVersion,
  type Migration,
} from "./schema.ts";
import type { SqlDb, SqlValue } from "./sql.ts";

const tables = async (db: SqlDb): Promise<string[]> =>
  (
    await db.all<{ name: string }>(
      "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
  ).map((r) => r.name);

describe("migrate", () => {
  test("a new file gets every table and the latest version", async () => {
    const db = memoryDb();
    expect(await schemaVersion(db)).toBe(0);
    expect(await migrate(db)).toEqual({ from: 0, to: SCHEMA_VERSION });
    expect(await schemaVersion(db)).toBe(SCHEMA_VERSION);
    expect(await tables(db)).toEqual(["point", "segment", "trip"]);
  });

  test("a file that is up to date is left as it is, with its trips", async () => {
    const db = memoryDb();
    await migrate(db);
    await db.run(
      "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m) VALUES (?, 'ended', 'manual', 1, 2, 3)",
      ["kept"],
    );
    expect(await migrate(db)).toEqual({ from: SCHEMA_VERSION, to: SCHEMA_VERSION });
    expect(await db.one("SELECT id FROM trip")).toEqual({ id: "kept" });
  });

  test("a file from a newer version of the app is refused and not written to", async () => {
    const db = memoryDb();
    await db.exec("PRAGMA user_version = 99");
    await expect(migrate(db)).rejects.toThrow(SchemaError);
    await expect(migrate(db)).rejects.toThrow(/newer version of Rutsa/);
    expect(await schemaVersion(db)).toBe(99);
    expect(await tables(db)).toEqual([]);
  });

  test("a migration that fails leaves the file as it was, also the migrations before it", async () => {
    const db = memoryDb();
    const failing: Migration[] = [
      {
        version: 1,
        description: "fine",
        up: async (tx) => {
          await tx.exec("CREATE TABLE first (x INTEGER)");
        },
      },
      {
        version: 2,
        description: "breaks",
        up: async (tx) => {
          await tx.exec("CREATE TABLE second (x INTEGER)");
          throw new Error("broken migration");
        },
      },
    ];
    await expect(migrate(db, failing)).rejects.toThrow("broken migration");
    expect(await schemaVersion(db)).toBe(0);
    expect(await tables(db)).toEqual([]);
  });

  test("a file of version 1 keeps its trips and points, and its recorded trips get their segments", async () => {
    const db = memoryDb();
    expect(await migrate(db, MIGRATIONS.slice(0, 1))).toEqual({ from: 0, to: 1 });
    // An ended trip that was paused once (two segments), a paused one and one added by hand.
    await db.run(
      "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m, segment) VALUES ('a', 'ended', 'recorded', 1000, 9000, 120, 2)",
    );
    await db.run(
      "INSERT INTO trip (id, state, source, started_at, segment) VALUES ('p', 'paused', 'recorded', 10000, 1)",
    );
    await db.run(
      "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m) VALUES ('m', 'ended', 'manual', 20000, 30000, 5000)",
    );
    await db.run(
      "INSERT INTO point (trip_id, seq, segment, t, lat, lon) VALUES ('a', 1, 1, 1000, 59.3, 18), ('a', 2, 2, 8000, 59.4, 18)",
    );

    expect(await migrate(db)).toEqual({ from: 1, to: SCHEMA_VERSION });
    expect(await schemaVersion(db)).toBe(2);
    expect(
      await db.all("SELECT trip_id, segment, started_at FROM segment ORDER BY trip_id, segment"),
    ).toEqual([
      { trip_id: "a", segment: 1, started_at: 1000 },
      { trip_id: "a", segment: 2, started_at: 1000 },
      { trip_id: "p", segment: 1, started_at: 10000 },
    ]);
    expect(await db.all("SELECT id, distance_m FROM trip ORDER BY id")).toEqual([
      { id: "a", distance_m: 120 },
      { id: "m", distance_m: 5000 },
      { id: "p", distance_m: null },
    ]);
    expect(await db.one("SELECT COUNT(*) AS n FROM point")).toEqual({ n: 2 });
  });

  test("migrations must be numbered 1, 2, 3 ... in order", async () => {
    const step = { description: "x", up: async () => {} };
    await expect(migrate(memoryDb(), [{ ...step, version: 2 }])).rejects.toThrow(SchemaError);
    await expect(
      migrate(memoryDb(), [
        { ...step, version: 1 },
        { ...step, version: 3 },
      ]),
    ).rejects.toThrow(/in order/);
  });
});

describe("the rules of the tables", () => {
  let db: SqlDb;

  beforeEach(async () => {
    db = memoryDb();
    await migrate(db);
  });

  const trip = (values: Record<string, SqlValue>) => {
    const row: Record<string, SqlValue> = {
      id: "t1",
      state: "recording",
      source: "recorded",
      started_at: 1_000,
      ...values,
    };
    const columns = Object.keys(row);
    return db.run(
      `INSERT INTO trip (${columns.join(", ")}) VALUES (${columns.map(() => "?").join(", ")})`,
      Object.values(row),
    );
  };

  const point = (values: Record<string, SqlValue>) => {
    const row: Record<string, SqlValue> = {
      trip_id: "t1",
      seq: 1,
      segment: 1,
      t: 1_000,
      lat: 59.33,
      lon: 18.06,
      ...values,
    };
    const columns = Object.keys(row);
    return db.run(
      `INSERT INTO point (${columns.join(", ")}) VALUES (${columns.map(() => "?").join(", ")})`,
      Object.values(row),
    );
  };

  test("deleting a trip deletes its points", async () => {
    await trip({});
    await point({ seq: 1 });
    await point({ seq: 2 });
    await db.run("DELETE FROM trip WHERE id = ?", ["t1"]);
    expect(await db.one("SELECT COUNT(*) AS n FROM point")).toEqual({ n: 0 });
  });

  test("a point needs a trip that exists", async () => {
    await expect(point({ trip_id: "nobody" })).rejects.toThrow();
  });

  test("only one trip is recording or paused at a time; ended ones are not limited", async () => {
    await trip({ id: "a", state: "recording" });
    await expect(trip({ id: "b", state: "recording" })).rejects.toThrow();
    await expect(trip({ id: "b", state: "paused" })).rejects.toThrow();
    await trip({ id: "c", state: "ended", ended_at: 2_000, distance_m: 0 });
    await trip({ id: "d", state: "ended", ended_at: 2_000, distance_m: 0 });
  });

  test("a state or a source that is not in the list is refused", async () => {
    await expect(trip({ state: "driving" })).rejects.toThrow();
    await expect(trip({ source: "imported" })).rejects.toThrow();
  });

  test("an ended trip has an end after its start, and only an ended trip has an end", async () => {
    await expect(trip({ state: "ended" })).rejects.toThrow();
    await expect(trip({ state: "recording", ended_at: 2_000 })).rejects.toThrow();
    await expect(trip({ state: "ended", ended_at: 500 })).rejects.toThrow();
  });

  test("a trip added by hand is ended and has a distance", async () => {
    await expect(trip({ source: "manual", state: "recording" })).rejects.toThrow();
    await expect(trip({ source: "manual", state: "ended", ended_at: 2_000 })).rejects.toThrow();
    await trip({ source: "manual", state: "ended", ended_at: 2_000, distance_m: 12_400 });
  });

  test("a negative distance or a segment below 1 is refused", async () => {
    await expect(trip({ distance_m: -1 })).rejects.toThrow();
    await expect(trip({ segment: 0 })).rejects.toThrow();
  });

  test("a point is stored once per sequence number and has a position on the Earth", async () => {
    await trip({});
    await point({ seq: 1 });
    await expect(point({ seq: 1 })).rejects.toThrow();
    await expect(point({ seq: 2, lat: 91 })).rejects.toThrow();
    await expect(point({ seq: 2, lon: -181 })).rejects.toThrow();
    await expect(point({ seq: 2, accuracy_m: -5 })).rejects.toThrow();
  });

  test("a segment belongs to a trip that exists, once per number, from 1, and goes with its trip", async () => {
    const segment = (values: SqlValue[]) =>
      db.run("INSERT INTO segment (trip_id, segment, started_at) VALUES (?, ?, ?)", values);
    await expect(segment(["nobody", 1, 1_000])).rejects.toThrow();
    await trip({});
    await segment(["t1", 1, 1_000]);
    await expect(segment(["t1", 1, 2_000])).rejects.toThrow();
    await expect(segment(["t1", 0, 2_000])).rejects.toThrow();
    await segment(["t1", 2, 2_000]);
    await db.run("DELETE FROM trip WHERE id = ?", ["t1"]);
    expect(await db.one("SELECT COUNT(*) AS n FROM segment")).toEqual({ n: 0 });
  });

  test("a column takes only its own type (STRICT)", async () => {
    await expect(trip({ started_at: "yesterday" })).rejects.toThrow();
  });
});

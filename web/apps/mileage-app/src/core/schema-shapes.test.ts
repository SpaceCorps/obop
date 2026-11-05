import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "./host-state.ts";
import { SchemaError, ensureModules, type SchemaModule } from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { MILEAGE_SCHEMA } from "./sql-store.ts";
import { emptyLegacyFile, legacyFile } from "../test-support/legacy-file.ts";
import { probeOlderBuild } from "../test-support/older-build-probe.ts";

// Every way to write a new column that a small grammar gives, put on a table of the first build
// through the manager, and the first build's own statements run on what is left (the probe).
//
// The grammar is what the reader of new columns reads: a name, a type, and the words NOT NULL, NULL
// and DEFAULT with a value, in any order, with repeats. The shapes are made here and are the same
// every time. Two things are held to them:
//
//   - the one thing the manager must never get wrong: when it lets a column through, the first
//     build still works on the file, with rows in it and without, and the rows are still there;
//   - what it lets through is what the grammar says is plain, by a rule written here again, in
//     terms of the pieces and not of the text: at most one DEFAULT, and a NOT NULL column has a
//     default that is not NULL, and no default that is the current time. (Shapes that SQLite
//     itself refuses, as it refuses that one on a table that has rows, are not the manager's to
//     judge; on an empty table SQLite accepts it, and the manager refuses it all the same.)
//
// This is a sample of the shapes, not all of them, and the oracle is the probe on SQLite as Node
// has it. The grammar is this file's own: the generator of the fuzz that found the second-DEFAULT
// case is not in the repository, so the number of shapes here (990 on the trips, a fifth of that on
// the points and the batches) is not that fuzz's.

const NAMES = ["extra", `"two words"`, "[b]"];
const TYPES = ["", "TEXT", "INTEGER", "VARCHAR(10)", "DECIMAL(10, 2)", "DOUBLE PRECISION"];
const PIECES = [
  "NOT NULL",
  "NULL",
  "DEFAULT 'a'",
  "DEFAULT 0",
  "DEFAULT -1",
  "DEFAULT NULL",
  "DEFAULT TRUE",
  "DEFAULT CURRENT_TIMESTAMP",
  "DEFAULT x'00'",
];

interface Shape {
  /** The column definition, as written after ADD COLUMN. */
  text: string;
  pieces: string[];
}

/** All sequences of up to `length` pieces, shortest first. */
function sequences(length: number): string[][] {
  let level: string[][] = [[]];
  const all: string[][] = [[]];
  for (let n = 0; n < length; n++) {
    level = level.flatMap((head) => PIECES.map((piece) => [...head, piece]));
    all.push(...level);
  }
  return all;
}

const write = (name: string, type: string, pieces: string[]): Shape => ({
  text: [name, type, ...pieces].filter((part) => part !== "").join(" "),
  pieces,
});

/**
 * Every sequence of up to three pieces, each with a name and a type taken in turn, and then every
 * sequence of up to one piece with every name and every type.
 */
function shapes(): Shape[] {
  const out = new Map<string, Shape>();
  sequences(3).forEach((pieces, i) => {
    const shape = write(NAMES[i % NAMES.length]!, TYPES[i % TYPES.length]!, pieces);
    out.set(shape.text, shape);
  });
  for (const name of NAMES) {
    for (const type of TYPES) {
      for (const pieces of sequences(1)) {
        const shape = write(name, type, pieces);
        out.set(shape.text, shape);
      }
    }
  }
  return [...out.values()];
}

/**
 * What the grammar says: at most one DEFAULT, and NOT NULL only with a default that is not NULL,
 * and no default that is the time of the insert (SQLite refuses it when the table has rows and
 * accepts it when it has none, so it is refused for every table).
 */
function plain({ pieces }: Shape): boolean {
  const defaults = pieces.filter((piece) => piece.startsWith("DEFAULT"));
  if (defaults.length > 1) return false;
  if (defaults.some((piece) => piece.startsWith("DEFAULT CURRENT_"))) return false;
  if (!pieces.includes("NOT NULL")) return true;
  return defaults.length === 1 && defaults[0] !== "DEFAULT NULL";
}

function mileageAt2(statement: string): SchemaModule {
  return {
    ...MILEAGE_SCHEMA,
    version: 2,
    async migrate(tx, from) {
      await MILEAGE_SCHEMA.migrate(tx, from);
      if (from < 2) await tx.exec(statement);
    },
  };
}

/** The rows of the first layout, by the columns it had. */
const dump = async (db: SqlDb) => ({
  trip: await db.all(
    "SELECT client_trip_id, purpose, started_at, created_ms FROM trip ORDER BY client_trip_id",
  ),
  point: await db.all("SELECT client_trip_id, seq, t, lat, lon FROM point ORDER BY 1, 2"),
  batch: await db.all("SELECT client_trip_id, first_seq, last_seq, sent FROM batch ORDER BY 1, 2"),
  meta: await db.all("SELECT key, value FROM meta ORDER BY key"),
});

const ALL = shapes();

/** Runs the shapes on a table and answers what went wrong, and how many were let through. */
async function run(table: "trip" | "point" | "batch", sample: Shape[]) {
  const wrong: string[] = [];
  let accepted = 0;
  let refused = 0;
  let sqlite = 0;
  for (const shape of sample) {
    const statement = `ALTER TABLE ${table} ADD COLUMN ${shape.text}`;
    for (const start of [emptyLegacyFile, legacyFile]) {
      // What SQLite says to the statement on its own.
      const alone = await (await start()).exec(statement).then(
        () => undefined,
        (e: unknown) => e,
      );
      const db = await start();
      const before = JSON.stringify(await dump(db));
      const error = await ensureModules(db, [HOST_SCHEMA, mileageAt2(statement)]).catch(
        (e: unknown) => e,
      );
      if (JSON.stringify(await dump(db)) !== before && error !== undefined) {
        wrong.push(`${shape.text}: not undone`);
      }
      if (alone !== undefined) {
        // SQLite's own refusal comes out as it is, and it is not the manager's.
        if (error === undefined || error instanceof SchemaError)
          wrong.push(`${shape.text}: sqlite`);
        sqlite++;
      } else if (error === undefined) {
        accepted++;
        const probe = await probeOlderBuild(db);
        if (!probe.ok) wrong.push(`${shape.text}: the first build breaks at ${probe.step}`);
        if (JSON.stringify(await dump(db)) !== before) wrong.push(`${shape.text}: rows changed`);
        if (!plain(shape)) wrong.push(`${shape.text}: let through`);
      } else if (error instanceof SchemaError) {
        refused++;
        if (plain(shape)) wrong.push(`${shape.text}: refused`);
      } else {
        wrong.push(`${shape.text}: ${error instanceof Error ? error.message : "an error"}`);
      }
    }
  }
  return { wrong, accepted, refused, sqlite };
}

describe("every column that the grammar gives, put on a table of the first build", () => {
  test("the shapes are the same every time, and there are enough of them", () => {
    expect(ALL.length).toBe(shapes().length);
    expect(ALL.length).toBe(990);
    expect(new Set(ALL.map((shape) => shape.text)).size).toBe(ALL.length);
  });

  test("on the trips, all of them, in a file with rows and in one without", async () => {
    const { wrong, accepted, refused, sqlite } = await run("trip", ALL);
    expect(wrong).toEqual([]);
    expect(accepted + refused + sqlite).toBe(2 * ALL.length);
    expect(accepted).toBeGreaterThan(0);
    expect(refused).toBeGreaterThan(0);
  }, 120_000);

  test.each(["point", "batch"] as const)(
    "on the %s, every fifth of them",
    async (table) => {
      const sample = ALL.filter((_, i) => i % 5 === 0);
      expect(sample.length).toBe(198);
      const { wrong, accepted, refused } = await run(table, sample);
      expect(wrong).toEqual([]);
      expect(accepted).toBeGreaterThan(0);
      expect(refused).toBeGreaterThan(0);
    },
    120_000,
  );
});

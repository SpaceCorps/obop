import type { SqlDb, SqlTx } from "./sql.ts";

/**
 * The tables of the trip store and how a database on a phone is brought up to date.
 *
 * The version of the tables is SQLite's own `PRAGMA user_version`: 0 in a new file, then the
 * version of the last migration that ran. A newer build adds a migration with the next number; it
 * runs once, on the first start of that build, and keeps every trip already on the phone. A file
 * whose version is higher than this build knows (the app was downgraded) is refused rather than
 * written to.
 *
 * Every table is STRICT (a column only takes its own type) and the rules that keep a trip sound
 * are CHECK constraints, so a bug in the store cannot write a trip that makes no sense. Foreign
 * keys must be switched on for each connection (`PRAGMA foreign_keys = ON`, see
 * `test-support/memory-db.ts` and `native/database.ts`); without it deleting a trip would leave
 * its points behind.
 *
 * After the idea of `mileage-app` (`src/core/schema.ts`): a version and ordered migrations, without
 * its module manager, which serves several modules in one host database.
 */

export interface Migration {
  /** 1, 2, 3 ... with no gaps. */
  readonly version: number;
  readonly description: string;
  /** Runs inside the transaction of `migrate`; only the `tx` given may be used. */
  up(tx: SqlTx): Promise<void>;
}

export const MIGRATIONS: readonly Migration[] = [
  {
    version: 1,
    description: "trips and their points",
    up: async (tx) => {
      await tx.exec(`
        CREATE TABLE trip (
          id TEXT PRIMARY KEY NOT NULL,
          state TEXT NOT NULL CHECK (state IN ('recording', 'paused', 'ended')),
          source TEXT NOT NULL CHECK (source IN ('recorded', 'manual')),
          started_at INTEGER NOT NULL,
          ended_at INTEGER,
          distance_m REAL CHECK (distance_m IS NULL OR distance_m >= 0),
          segment INTEGER NOT NULL DEFAULT 1 CHECK (segment >= 1),
          from_text TEXT,
          to_text TEXT,
          note TEXT,
          -- An ended trip has an end, and only an ended trip has one.
          CHECK ((state = 'ended') = (ended_at IS NOT NULL)),
          CHECK (ended_at IS NULL OR ended_at >= started_at),
          -- A trip added by hand is ended when it is saved and has the distance the driver typed.
          CHECK (source = 'recorded' OR (state = 'ended' AND distance_m IS NOT NULL))
        ) STRICT
      `);
      // At most one trip is recording or paused. The indexed expression is true for every row the
      // index holds, so a second such row has the same key and is refused.
      await tx.exec(`
        CREATE UNIQUE INDEX trip_one_active ON trip ((state IN ('recording', 'paused')))
        WHERE state IN ('recording', 'paused')
      `);
      // The list by day and the totals of a day or a month read trips by their start.
      await tx.exec("CREATE INDEX trip_started ON trip (started_at)");
      await tx.exec(`
        CREATE TABLE point (
          trip_id TEXT NOT NULL REFERENCES trip (id) ON DELETE CASCADE,
          seq INTEGER NOT NULL CHECK (seq >= 1),
          segment INTEGER NOT NULL CHECK (segment >= 1),
          t INTEGER NOT NULL,
          lat REAL NOT NULL CHECK (lat BETWEEN -90 AND 90),
          lon REAL NOT NULL CHECK (lon BETWEEN -180 AND 180),
          accuracy_m REAL CHECK (accuracy_m IS NULL OR accuracy_m >= 0),
          speed_mps REAL CHECK (speed_mps IS NULL OR speed_mps >= 0),
          -- The same position cannot be stored twice in a trip.
          PRIMARY KEY (trip_id, seq)
        ) STRICT, WITHOUT ROWID
      `);
      // At Stop the distance is computed segment by segment.
      await tx.exec("CREATE INDEX point_segment ON point (trip_id, segment)");
    },
  },
];

/** The version of the tables this build works with. */
export const SCHEMA_VERSION = latestVersion(MIGRATIONS);

/** A database this build cannot use, or a list of migrations that is not in order. */
export class SchemaError extends Error {
  override name = "SchemaError";
}

function latestVersion(migrations: readonly Migration[]): number {
  return migrations.length === 0 ? 0 : (migrations[migrations.length - 1]?.version ?? 0);
}

function checkOrder(migrations: readonly Migration[]): void {
  migrations.forEach((m, i) => {
    if (m.version !== i + 1) {
      throw new SchemaError(
        `migration ${i + 1} has version ${m.version}; versions must be 1, 2, 3 ... in order`,
      );
    }
  });
}

/** `PRAGMA user_version` of the file. */
export async function schemaVersion(q: SqlTx): Promise<number> {
  return (await q.one<{ user_version: number }>("PRAGMA user_version"))?.user_version ?? 0;
}

/**
 * Brings the file to the latest version: all missing migrations run in one transaction, so a
 * failure leaves the file as it was. Running it on a file that is up to date changes nothing.
 */
export async function migrate(
  db: SqlDb,
  migrations: readonly Migration[] = MIGRATIONS,
): Promise<{ from: number; to: number }> {
  checkOrder(migrations);
  const latest = latestVersion(migrations);
  return db.transaction(async (tx) => {
    const from = await schemaVersion(tx);
    if (from > latest) {
      throw new SchemaError(
        `the trips on this phone were saved by a newer version of Rutsa (tables version ${from}); this version knows up to ${latest}`,
      );
    }
    for (const m of migrations.slice(from)) {
      await m.up(tx);
      // A whole number from the list above, never text from outside.
      await tx.exec(`PRAGMA user_version = ${m.version}`);
    }
    return { from, to: latest };
  });
}

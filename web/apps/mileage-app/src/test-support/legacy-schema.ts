import type { SqlDb } from "../core/sql.ts";

/**
 * The schema and the migration of the first builds, kept as they were. A copy, not an import:
 * `core/sql-store.ts` was split into the host's table and the outbox's tables, and the point of this
 * file is to be the old code that a phone keeps running after it has been updated and then
 * downgraded, and the old text that a file made by an old build was made from. Nothing in it may
 * change, and `schema-upgrade.test.ts` holds its text to a checksum. The only differences from the
 * file it was copied from (`sql-store.ts` of the commit before the schema manager) are the imports
 * above, the `export` on `SCHEMA`, and that the pointer name `ACTIVE`, which it does not use, is left out.
 */

/** Bump with a migration step in `migrate` when a table changes. */
export const SCHEMA_VERSION = 1;

export const SCHEMA = `
CREATE TABLE IF NOT EXISTS trip (
  client_trip_id TEXT PRIMARY KEY NOT NULL,
  server_id TEXT,
  origin TEXT NOT NULL DEFAULT 'device',
  purpose TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  create_attempted INTEGER NOT NULL DEFAULT 0,
  stop_requested INTEGER NOT NULL DEFAULT 0,
  expected_points INTEGER,
  last_seq INTEGER,
  end_done INTEGER NOT NULL DEFAULT 0,
  submit_requested INTEGER NOT NULL DEFAULT 0,
  submit_done INTEGER NOT NULL DEFAULT 0,
  discard_reason TEXT,
  discard_done INTEGER NOT NULL DEFAULT 0,
  server_state TEXT,
  distance_m TEXT,
  integrity TEXT,
  reject_reason TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  problem_step TEXT,
  problem_code TEXT,
  problem_message TEXT,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS point (
  client_trip_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  t INTEGER NOT NULL,
  lat TEXT NOT NULL,
  lon TEXT NOT NULL,
  accuracy_m TEXT,
  speed_mps TEXT,
  PRIMARY KEY (client_trip_id, seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS batch (
  client_trip_id TEXT NOT NULL,
  first_seq INTEGER NOT NULL,
  last_seq INTEGER NOT NULL,
  sent INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (client_trip_id, first_seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
) WITHOUT ROWID;
`;

/** Creates the tables on a new database. A database of a newer app is refused, not changed. */
export async function migrate(db: SqlDb): Promise<void> {
  const row = await db.one<{ user_version: number }>("PRAGMA user_version");
  const version = Number(row?.user_version ?? 0);
  if (version > SCHEMA_VERSION) {
    throw new Error(`The database is from a newer version of the app (schema ${version})`);
  }
  if (version < 1) {
    await db.transaction(async (tx) => {
      await tx.exec(SCHEMA);
      await tx.exec(`PRAGMA user_version = ${SCHEMA_VERSION}`);
    });
  }
}

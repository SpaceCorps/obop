import type { SqlDb } from "../core/sql.ts";
import { migrate as legacyMigrate } from "./legacy-schema.ts";
import { memoryDb } from "./memory-db.ts";

/** A made-up moment for the data in the files below. */
export const T0 = Date.parse("2026-10-15T09:00:00Z");

/** A file as the first build leaves it after a few days of use. All data here is made up. */
export async function seed(db: SqlDb): Promise<void> {
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

/** A file made by the first build's own migration, with the rows of `seed`. */
export async function legacyFile(): Promise<SqlDb> {
  const db = await emptyLegacyFile();
  await seed(db);
  return db;
}

/** A file made by the first build's own migration, with no rows. */
export async function emptyLegacyFile(): Promise<SqlDb> {
  const db = memoryDb();
  await legacyMigrate(db);
  return db;
}

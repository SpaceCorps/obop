import { scopedMeta, type MetaAccess, type SchemaModule } from "./schema.ts";
import type { SqlDb } from "./sql.ts";

/**
 * What the host keeps in the database file for every app built on it, apart from the app's own
 * tables: whose trips the file holds, whether the person has read the location disclosure, and the
 * note that a file of a restored backup was replaced. They are rows of the table `meta`, which the
 * first build made, and they are in the host's own module (`host` in `module_schema`) with their
 * own definition of the table and their own way in, so that the host does not reach them through
 * an app's store.
 *
 * Nothing was moved or copied out of the app's store when this was made: the rows are where the
 * first build put them, under the keys it gave them, and a build before this one reads and writes
 * them in the same place. A copy would be two places for one fact, and an older build would go on
 * reading the old one.
 */

/** Whose trips the file holds (`core/identity.ts`). */
export const META_OWNER = "owner";
/** When the person accepted the disclosure before location is used. */
export const DISCLOSURE_META = "location_disclosure_accepted";
/** The note in the file that says a database file was found that no key fits and was replaced. */
export const META_STALE_FILE_RECOVERED = "stale_file_recovered_at";

/**
 * The table `meta`, with the text of its first definition: the file keeps this text, so it is
 * written out the way it always was.
 */
const CREATE_META = `CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
) WITHOUT ROWID;`;

/**
 * The host's tables. `meta` and the three keys above belong to it although their names do not
 * start with `host`: they are the first layout's, and a file that an earlier build made has them.
 * Version 1 is the version every file of layout 1 has.
 */
export const HOST_SCHEMA: SchemaModule = {
  id: "host",
  version: 1,
  legacyVersion: 1,
  legacyTables: ["meta"],
  legacyMetaKeys: [META_OWNER, DISCLOSURE_META, META_STALE_FILE_RECOVERED],
  async migrate(tx, from) {
    if (from < 1) await tx.exec(CREATE_META);
  },
};

/** The host's rows of `meta`: any other key throws (`scopedMeta`). */
export type HostState = MetaAccess;

/** The way in to the host's rows. It reads and writes the table with the statements the app's store uses for it. */
export function createHostState(db: SqlDb): HostState {
  return scopedMeta(
    {
      async getMeta(key) {
        return (
          (await db.one<{ value: string }>("SELECT value FROM meta WHERE key = ?", [key]))?.value ??
          null
        );
      },
      async setMeta(key, value) {
        await db.transaction(async (tx) => {
          if (value === null) await tx.run("DELETE FROM meta WHERE key = ?", [key]);
          else
            await tx.run(
              "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
              [key, value],
            );
        });
      },
    },
    HOST_SCHEMA,
  );
}

import type { SqlDb } from "../core/sql.ts";

/**
 * A database that records what is done to it: every statement that can change something, and every
 * transaction. Reads (`all`, `one`) are not recorded, so an empty log means the file was only read.
 */
export function watched(db: SqlDb): { spy: SqlDb; log: string[] } {
  const log: string[] = [];
  const spy: SqlDb = {
    ...db,
    exec: (sql) => {
      log.push(`exec ${sql.slice(0, 40)}`);
      return db.exec(sql);
    },
    run: (sql, params) => {
      log.push(`run ${sql.slice(0, 40)}`);
      return db.run(sql, params);
    },
    transaction: (fn) => {
      log.push("transaction");
      return db.transaction(fn);
    },
  };
  return { spy, log };
}

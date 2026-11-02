import { DatabaseSync } from "node:sqlite";
import { createSqlDb, type RawDb, type SqlDb, type SqlValue } from "../core/sql.ts";

/**
 * A real SQLite database in memory for the tests, behind the same `SqlDb` the phone uses over
 * expo-sqlite. (SQLCipher is not in it: encryption is the phone's, see `native/database.ts`.)
 */
export function memoryDb(): SqlDb {
  const sqlite = new DatabaseSync(":memory:");
  const raw: RawDb = {
    exec: async (sql) => {
      sqlite.exec(sql);
    },
    run: async (sql, params) => {
      sqlite.prepare(sql).run(...(params as SqlValue[]));
    },
    all: async (sql, params) =>
      sqlite
        .prepare(sql)
        .all(...(params as SqlValue[]))
        .map((row) => ({ ...row }) as Record<string, SqlValue>),
  };
  return createSqlDb(raw);
}

import { DatabaseSync } from "node:sqlite";
import { createSqlDb, type RawDb, type SqlDb, type SqlValue } from "../core/sql.ts";

/**
 * The `SqlDb` the phone uses, over an open `node:sqlite` connection (in memory or a file). Foreign
 * keys are switched on, as on the phone (`core/schema.ts`): SQLite has them off on every new
 * connection.
 */
export function sqlDbOver(sqlite: DatabaseSync): SqlDb {
  sqlite.exec("PRAGMA foreign_keys = ON");
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

/**
 * A real SQLite database in memory for the tests, behind the same `SqlDb` the phone uses over
 * expo-sqlite. SQLCipher is not in it: the encryption is the phone's (`native/database.ts`).
 */
export function memoryDb(): SqlDb {
  return sqlDbOver(new DatabaseSync(":memory:"));
}

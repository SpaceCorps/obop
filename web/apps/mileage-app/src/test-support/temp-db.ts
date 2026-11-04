import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { SqlDb } from "../core/sql.ts";
import { sqlDbOver } from "./memory-db.ts";

/**
 * One database file in a folder of its own under the temporary folder of the machine, with any
 * number of connections to it: what the screens and the location task are (two connections to one
 * file). `close` removes the folder. The connections do not wait for a lock (a second writer gets
 * "database is locked" at once), because these connections are synchronous and a wait would never
 * end.
 */
export function tempDatabase() {
  const dir = mkdtempSync(join(tmpdir(), "mileage-db-"));
  const path = join(dir, "outbox.db");
  const connections: DatabaseSync[] = [];
  return {
    path,
    connect(): { db: SqlDb; sqlite: DatabaseSync } {
      const sqlite = new DatabaseSync(path);
      connections.push(sqlite);
      return { db: sqlDbOver(sqlite), sqlite };
    },
    close(): void {
      for (const c of connections) {
        try {
          c.close();
        } catch {
          // Closed already.
        }
      }
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

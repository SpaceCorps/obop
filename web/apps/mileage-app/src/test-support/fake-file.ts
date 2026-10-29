import { DatabaseSync } from "node:sqlite";
import type { FileHandle, OpenPorts } from "../core/open-outbox.ts";
import type { SqlValue } from "../core/sql.ts";

/**
 * A database file that behaves like an SQLCipher file for the parts that `openOutbox` depends on:
 * it is encrypted with the key it was made with, a statement that reads it with another key fails
 * with SQLite's "file is not a database", the file cannot be deleted while a handle is open
 * (expo-sqlite throws then), and the SQL itself is real (`node:sqlite`, in memory).
 */
export class FakeDatabaseFile {
  /** The key the file was made with; `null` when there is no file. */
  keyOfFile: string | null = null;
  sqlite: DatabaseSync | null = null;
  /** What `PRAGMA cipher_version` answers; `null` is plain SQLite, which has no such pragma. */
  cipherVersion: string | null = "4.17.0 community";
  opens = 0;
  closes = 0;
  deletes = 0;
  private openHandles = 0;
  /** An error that the next `openFile` throws (a disk error), once. */
  failNextOpenWith: Error | null = null;

  /** A file that came back with a restore of a backup: encrypted with a key that is not on this phone. */
  plantStaleFile(key = "f".repeat(64)): void {
    this.keyOfFile = key;
    this.sqlite = new DatabaseSync(":memory:");
    this.sqlite.exec("CREATE TABLE secret_of_the_old_phone (x TEXT)");
  }

  /** The file was made with `key` and holds a database that `migrate` has run on. */
  async makeFile(key: string): Promise<void> {
    const handle = await this.openFile();
    await handle.exec(`PRAGMA key = "x'${key}'"`);
    const { migrate } = await import("../core/sql-store.ts");
    const { createSqlDb } = await import("../core/sql.ts");
    await migrate(createSqlDb(handle));
    await handle.close();
  }

  async openFile(): Promise<FileHandle> {
    if (this.failNextOpenWith) {
      const error = this.failNextOpenWith;
      this.failNextOpenWith = null;
      throw error;
    }
    this.opens += 1;
    this.openHandles += 1;
    let key: string | null = null;
    let closed = false;
    const alive = () => {
      if (closed) throw new Error("The database is closed");
    };
    // Every statement that reads the file goes through here.
    const unlock = (): DatabaseSync => {
      alive();
      if (this.keyOfFile === null) {
        this.keyOfFile = key;
        this.sqlite = new DatabaseSync(":memory:");
      } else if (this.keyOfFile !== key) {
        throw new Error(
          "Call to function 'NativeDatabase.execAsync' has been rejected. Caused by: Error code 26: file is not a database",
        );
      }
      return this.sqlite!;
    };
    return {
      exec: async (sql) => {
        alive();
        const set = /^PRAGMA key = "x'([0-9a-f]+)'"$/.exec(sql);
        if (set) {
          key = set[1]!;
          return;
        }
        if (sql.startsWith("PRAGMA busy_timeout")) return;
        // Setting the journal mode reads the header of the file.
        if (sql.startsWith("PRAGMA journal_mode")) {
          unlock();
          return;
        }
        unlock().exec(sql);
      },
      run: async (sql, params) => {
        unlock()
          .prepare(sql)
          .run(...(params as SqlValue[]));
      },
      all: async (sql, params) => {
        alive();
        if (sql === "PRAGMA cipher_version") {
          return this.cipherVersion === null ? [] : [{ cipher_version: this.cipherVersion }];
        }
        return unlock()
          .prepare(sql)
          .all(...(params as SqlValue[]))
          .map((row) => ({ ...row }) as Record<string, SqlValue>);
      },
      close: async () => {
        if (closed) return;
        closed = true;
        this.closes += 1;
        this.openHandles -= 1;
      },
    };
  }

  async deleteFile(): Promise<void> {
    if (this.openHandles > 0) throw new Error("Cannot delete a database that is open");
    this.deletes += 1;
    this.keyOfFile = null;
    this.sqlite = null;
  }
}

/** The Keychain as the ports see it, with a record of what was done to it. */
export class FakeSecrets {
  items = new Map<string, string>();
  log: string[] = [];
  get = async (name: string) => this.items.get(name) ?? null;
  set = async (name: string, value: string) => {
    this.log.push(`set ${name}`);
    this.items.set(name, value);
  };
  delete = async (name: string) => {
    this.log.push(`delete ${name}`);
    this.items.delete(name);
  };
}

export function fakePorts(
  file: FakeDatabaseFile,
  secrets: FakeSecrets,
  extra: { key?: string; now?: number } = {},
): OpenPorts {
  return {
    secrets,
    randomKey: async () => extra.key ?? "a".repeat(64),
    openFile: () => file.openFile(),
    deleteFile: () => file.deleteFile(),
    now: () => extra.now ?? Date.parse("2026-10-30T10:00:00Z"),
  };
}

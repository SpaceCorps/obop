import { createSqlDb, type RawDb, type SqlDb } from "./sql.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import type { OutboxStore } from "./store.ts";
import { instant } from "./values.ts";

/**
 * Opening the encrypted database: the key, the file, and the two things that are only true on iOS.
 * The phone's parts (expo-sqlite, the Keychain, a random source) come in as `OpenPorts`, so the
 * whole procedure runs in `vp test` with a fake file; `native/database.ts` wires the real ones.
 *
 * Why iOS needs more than Android here (docs/spec/mobile.md 8.8):
 *
 * - The key lives in the Keychain with the class "this device only" (`native/keychain.ts`), and the
 *   database file lives in the app's Documents folder, which an iCloud or iTunes backup carries to
 *   another phone. After such a restore the file is there and the key is not: a key made now does
 *   not fit it. Android has no such case (its backup is off). The file cannot be read by anyone,
 *   so it is removed and a new one is made, and the driver is told (`META_STALE_FILE_RECOVERED`).
 *   This is done only for a key that was made in this very start, or one that is still `pending`
 *   (stored by an earlier start and not yet proven by an open). A key that has already opened a file
 *   is never followed by a deletion: the file is left as it is and the error is shown, as on
 *   Android. A pending key has opened nothing yet, so it is followed by one.
 * - The Keychain survives an uninstall and the files of the app do not. After a reinstall the
 *   database is new and the server address and tokens are still there; a new database means a new
 *   installation, so the stored sign-in is dropped and the app starts signed out.
 */

/** The note in the outbox that says a database file was found that no key fits and was replaced. */
export const META_STALE_FILE_RECOVERED = "stale_file_recovered_at";

/** When the file of a restored backup was replaced, or `null` when there is nothing to tell (or the driver has read it). */
export function staleFileRecoveredAt(store: Pick<OutboxStore, "getMeta">): Promise<string | null> {
  return store.getMeta(META_STALE_FILE_RECOVERED);
}

/** The driver has read the note about the replaced file. */
export function dismissStaleFileNote(store: Pick<OutboxStore, "setMeta">): Promise<void> {
  return store.setMeta(META_STALE_FILE_RECOVERED, null);
}

/** An open database file. */
export interface FileHandle extends RawDb {
  close(): Promise<void>;
}

export interface OpenPorts {
  secrets: {
    get(name: string): Promise<string | null>;
    set(name: string, value: string): Promise<void>;
    delete(name: string): Promise<void>;
  };
  /** 32 random bytes as 64 lower case hex digits. */
  randomKey(): Promise<string>;
  /** Opens (creates) the database file. The file is not read until the key is set. */
  openFile(): Promise<FileHandle>;
  /** Removes the file and its side files. The file must be closed. */
  deleteFile(): Promise<void>;
  now(): number;
}

export interface OpenNames {
  /** Where the key of the database is kept. */
  databaseKey: string;
  /** The items of the signed-in session (server address, tokens). */
  session: readonly string[];
}

export interface OpenPolicy {
  recoverStaleFile: boolean;
  forgetSessionOnFreshDatabase: boolean;
}

/** The two rules above are for iOS. Android keeps the behaviour it had. */
export function policyFor(os: string): OpenPolicy {
  const ios = os === "ios";
  return { recoverStaleFile: ios, forgetSessionOnFreshDatabase: ios };
}

export interface Opened {
  db: SqlDb;
  /** The version of SQLCipher the database runs on. */
  cipherVersion: string;
  /** The database was new (an empty file, or none). */
  fresh: boolean;
  /** A file that no key fit was found and replaced. */
  recoveredStaleFile: boolean;
}

/** What SQLite says when the key does not fit the file (SQLITE_NOTADB, code 26). */
export function isNotADatabase(error: unknown): boolean {
  const text = error instanceof Error ? error.message : String(error);
  return /file is not a database|file is encrypted or is not a database|SQLITE_NOTADB|\bcode 26\b/i.test(
    text,
  );
}

interface OpenFile {
  file: FileHandle;
  db: SqlDb;
  cipherVersion: string;
  fresh: boolean;
}

async function openWithKey(ports: OpenPorts, key: string): Promise<OpenFile> {
  const file = await ports.openFile();
  try {
    // The key goes first; nothing else may read the file before it.
    await file.exec(`PRAGMA key = "x'${key}'"`);
    const version = await file.all("PRAGMA cipher_version", []);
    const cipherVersion = version[0]?.cipher_version;
    if (typeof cipherVersion !== "string" || cipherVersion === "") {
      // Plain SQLite would store the route in the clear. Fail closed.
      throw new Error("The database is not encrypted: this build has no SQLCipher");
    }
    await file.exec("PRAGMA busy_timeout = 5000");
    await file.exec("PRAGMA journal_mode = WAL");
    const db = createSqlDb(file);
    // A wrong key shows here as "file is not a database".
    const row = await db.one<{ user_version: number }>("PRAGMA user_version");
    const fresh = Number(row?.user_version ?? 0) === 0;
    await migrate(db);
    return { file, db, cipherVersion, fresh };
  } catch (error) {
    await file.close().catch(() => undefined);
    throw error;
  }
}

/**
 * Where a key is kept until a database has been opened with it for the first time. Where the
 * stale-file recovery is on, a key that was just made is stored as `pending:<key>` and made plain
 * after the first successful open. A start that finds a pending key knows that no file has been
 * opened with it yet and may still be stale; so a failed deletion, or a kill between the storing of
 * the key and the replacing of the file, is put right by the next start and not left as "file is
 * not a database" for good.
 */
const PENDING = /^pending:([0-9a-f]{64})$/;

export async function openOutbox(
  ports: OpenPorts,
  names: OpenNames,
  policy: OpenPolicy,
): Promise<Opened> {
  const known = await ports.secrets.get(names.databaseKey);
  let key: string;
  // The key has not been proven by an open yet: the file, if there is one, may be a stale one.
  let generated = false;
  const pending = known === null ? null : PENDING.exec(known);
  if (known !== null && /^[0-9a-f]{64}$/.test(known)) {
    key = known;
  } else if (pending !== null && policy.recoverStaleFile) {
    key = pending[1]!;
    generated = true;
  } else {
    key = await ports.randomKey();
    await ports.secrets.set(names.databaseKey, policy.recoverStaleFile ? `pending:${key}` : key);
    generated = true;
  }

  let opened: OpenFile;
  let recoveredStaleFile = false;
  try {
    opened = await openWithKey(ports, key);
  } catch (error) {
    if (!(generated && policy.recoverStaleFile && isNotADatabase(error))) throw error;
    await ports.deleteFile();
    recoveredStaleFile = true;
    opened = await openWithKey(ports, key);
  }

  try {
    if (opened.fresh && policy.forgetSessionOnFreshDatabase) {
      for (const name of names.session) await ports.secrets.delete(name);
    }
    if (recoveredStaleFile) {
      await createSqlStore(opened.db).setMeta(META_STALE_FILE_RECOVERED, instant(ports.now()));
    }
    // The key is proven now: it opened a file. Last, so that a kill before this line is met again.
    if (generated && policy.recoverStaleFile) await ports.secrets.set(names.databaseKey, key);
  } catch (error) {
    // The file is open and nobody gets the handle: close it, or a retry of the start (the caller
    // forgets a failed open) opens the file a second time.
    await opened.file.close().catch(() => undefined);
    throw error;
  }
  return {
    db: opened.db,
    cipherVersion: opened.cipherVersion,
    fresh: opened.fresh,
    recoveredStaleFile,
  };
}

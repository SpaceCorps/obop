import { SchemaError, migrate } from "./schema.ts";
import { createSqlDb, type RawDb, type SqlDb } from "./sql.ts";

/**
 * Opening the encrypted database on the phone: the key, the file, and the cases where they do not
 * fit. The phone's parts (expo-sqlite, the secure store, a random source) come in as `OpenPorts`,
 * so the whole procedure runs in `vp test` with a fake file (`test-support/fake-file.ts`);
 * `native/database.ts` wires the real ones.
 *
 * The key is 32 random bytes made on the first start, kept in the Keystore (Android) or the
 * Keychain (iOS), never next to the file. The rules:
 *
 * - No key and no file: a new key and a new database.
 * - A key and no file (iOS keeps the Keychain when the app is deleted, not its files): a new
 *   database with that key.
 * - A key that does not fit the file: an error, and the file is left alone. Trips that are deleted
 *   cannot come back; the cause may pass (a secure store that could not be read once).
 * - No usable key and a file (the file came back with a backup, its key did not): nobody can read
 *   that file, so it is deleted and a new one is made, and the caller is told
 *   (`recoveredStaleFile`). A new key is stored as `pending:<key>` until it has opened a file, so
 *   a start that is cut off between storing the key and opening the file is met again the same way.
 *   This is the only case in which a file is deleted.
 * - Plain SQLite (the build lacks SQLCipher): refused, the trips are never stored in the clear.
 *
 * After `openOutbox` of `mileage-app` (`src/core/open-outbox.ts`), without its session and server
 * rules. The stale file is recovered on both systems here: `mileage-app` does it on iOS only, and
 * keeps the trips on its server besides.
 */

/** An open database file. */
export interface FileHandle extends RawDb {
  close(): Promise<void>;
}

export interface OpenPorts {
  secrets: {
    get(name: string): Promise<string | null>;
    set(name: string, value: string): Promise<void>;
  };
  /** 32 random bytes as 64 lower case hex digits. */
  randomKey(): Promise<string>;
  /** Opens (creates) the database file. The file is not read until the key is set. */
  openFile(): Promise<FileHandle>;
  /** Removes the file and its side files. The file must be closed. */
  deleteFile(): Promise<void>;
}

export interface Opened {
  db: SqlDb;
  /** The version of SQLCipher the database runs on. */
  cipherVersion: string;
  /** The database was new: no tables yet before this start. */
  fresh: boolean;
  /** A file that no key fit was found and replaced by a new one. */
  recoveredStaleFile: boolean;
}

const KEY = /^[0-9a-f]{64}$/;
const PENDING = /^pending:([0-9a-f]{64})$/;

/** What SQLite says when the key does not fit the file (SQLITE_NOTADB, code 26). */
export function isNotADatabase(error: unknown): boolean {
  const text = error instanceof Error ? error.message : String(error);
  return /file is not a database|file is encrypted or is not a database|SQLITE_NOTADB|\bcode 26\b/i.test(
    text,
  );
}

/** The message of the refusal of a database without SQLCipher. */
export const NOT_ENCRYPTED = "The database is not encrypted: this build has no SQLCipher";

/**
 * Why the database could not be opened, as a word the catalogue has a sentence for, so that the
 * screen of the failure says it in the driver's words: a build without SQLCipher, a key that does
 * not fit the file (a lost key, a damaged file), a file from a newer version of the app, or
 * anything else (shown as the error says it).
 *
 * After `databaseProblem` of `mileage-app` (`src/core/open-outbox.ts`).
 */
export type DatabaseProblem = "no_cipher" | "wrong_key" | "newer" | "other";

export function databaseProblem(error: unknown): DatabaseProblem {
  const text = error instanceof Error ? error.message : String(error);
  if (text.startsWith(NOT_ENCRYPTED)) return "no_cipher";
  if (isNotADatabase(error)) return "wrong_key";
  if (error instanceof SchemaError && text.includes("newer version of Rutsa")) return "newer";
  return "other";
}

/**
 * Whether erasing the trips can help: with a key that does not fit or a cause the app does not
 * know, a new empty database makes the app work again. A file from a newer version of the app is
 * sound (installing that version again opens it), and a build without SQLCipher refuses the new
 * file as well, so neither offers to erase.
 */
export function canErase(problem: DatabaseProblem): boolean {
  return problem === "wrong_key" || problem === "other";
}

/**
 * Erases the trips on this phone when the database cannot be opened: the file and its side files
 * first, then the key, so that the next open makes a new key and a new, empty file. The file must
 * be closed (a failed open closes it). The phone holds the only copy of the trips, so they are
 * gone for good, which the screen says twice before it calls this. A deletion of the file that
 * fails leaves the key, so nothing that could still be read is made unreadable; a key that is not
 * deleted after the file was is harmless (the next open makes a new file with it).
 *
 * After `eraseOutbox` of `mileage-app` (`src/core/open-outbox.ts`).
 */
export async function eraseDatabaseAndKey(
  ports: { deleteFile(): Promise<void>; secrets: { delete(name: string): Promise<void> } },
  keyName: string,
): Promise<void> {
  await ports.deleteFile();
  await ports.secrets.delete(keyName);
}

/** The failures a test build can ask for (`EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE`). */
export const TEST_FAILURES = ["wrong_key", "newer", "no_cipher"] as const;

/**
 * TEMPORARY, removed before the stage is done: the error a test build pretends to get when it
 * opens the database, so that the screen of the failure can be tried on the emulator, where a
 * release build cannot be given a damaged file. `undefined` or empty is a normal build; a value
 * that is not one of `TEST_FAILURES` fails too, with a message that names it, so a typo shows.
 */
export function testFailure(value: string | undefined): Error | null {
  switch (value) {
    case undefined:
    case "":
      return null;
    case "wrong_key":
      return new Error("Error code 26: file is not a database (test build)");
    case "newer":
      return new SchemaError(
        "the trips on this phone were saved by a newer version of Rutsa (tables version 99, test build)",
      );
    case "no_cipher":
      return new Error(`${NOT_ENCRYPTED} (test build)`);
    default:
      return new Error(
        `EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE must be one of ${TEST_FAILURES.join(", ")}, got ${JSON.stringify(value)}`,
      );
  }
}

/** Closes a file while another error is on its way, so nothing the close does replaces it. */
async function closeQuietly(file: FileHandle): Promise<void> {
  try {
    await Promise.resolve().then(() => file.close());
  } catch {
    // The caller rethrows the error that matters.
  }
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
    // The key goes first; nothing else may read the file before it. It is 64 hex digits (KEY).
    await file.exec(`PRAGMA key = "x'${key}'"`);
    const version = await file.all("PRAGMA cipher_version", []);
    const cipherVersion = version[0]?.cipher_version;
    if (typeof cipherVersion !== "string" || cipherVersion === "") {
      // Plain SQLite would store the routes in the clear. Fail closed.
      throw new Error(NOT_ENCRYPTED);
    }
    await file.exec("PRAGMA busy_timeout = 5000");
    await file.exec("PRAGMA journal_mode = WAL");
    // Off on every new connection in SQLite; without it a deleted trip would leave its points.
    await file.exec("PRAGMA foreign_keys = ON");
    const db = createSqlDb(file);
    // A wrong key shows here as "file is not a database".
    const row = await db.one<{ user_version: number }>("PRAGMA user_version");
    const fresh = Number(row?.user_version ?? 0) === 0;
    await migrate(db);
    return { file, db, cipherVersion, fresh };
  } catch (error) {
    await closeQuietly(file);
    throw error;
  }
}

/** Opens the database with the key kept under `keyName`, making the key on the first start. */
export async function openDatabase(ports: OpenPorts, keyName: string): Promise<Opened> {
  const known = await ports.secrets.get(keyName);
  const pending = known === null ? null : PENDING.exec(known);
  let key: string;
  // The key has not opened a file yet: a file that is there may be one it cannot read.
  let unproven: boolean;
  if (known !== null && KEY.test(known)) {
    key = known;
    unproven = false;
  } else if (pending?.[1] !== undefined) {
    key = pending[1];
    unproven = true;
  } else {
    key = await ports.randomKey();
    await ports.secrets.set(keyName, `pending:${key}`);
    unproven = true;
  }

  let opened: OpenFile;
  let recoveredStaleFile = false;
  try {
    opened = await openWithKey(ports, key);
  } catch (error) {
    if (!(unproven && isNotADatabase(error))) throw error;
    await ports.deleteFile();
    recoveredStaleFile = true;
    opened = await openWithKey(ports, key);
  }

  if (unproven) {
    try {
      // The key is proven now: it opened a file. Last, so that a start cut off before it is met again.
      await ports.secrets.set(keyName, key);
    } catch (error) {
      // Nobody gets the handle: close it, or the next try opens the file a second time.
      await closeQuietly(opened.file);
      throw error;
    }
  }
  return {
    db: opened.db,
    cipherVersion: opened.cipherVersion,
    fresh: opened.fresh,
    recoveredStaleFile,
  };
}

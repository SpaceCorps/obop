import { describe, expect, test } from "vite-plus/test";
import { FakeDatabaseFile, FakeSecrets, fakePorts } from "../test-support/fake-file.ts";
import {
  META_STALE_FILE_RECOVERED,
  isNotADatabase,
  openOutbox,
  policyFor,
  type OpenNames,
} from "./open-outbox.ts";
import { createSqlStore } from "./sql-store.ts";

const NAMES: OpenNames = {
  databaseKey: "db-key",
  session: ["server", "tokens"],
};
const KEY = "a".repeat(64);
const OTHER = "b".repeat(64);
const IOS = policyFor("ios");
const ANDROID = policyFor("android");

/** A Keychain that holds a signed-in session, as after a sign-in. */
function signedIn(secrets: FakeSecrets) {
  secrets.items.set("server", '{"baseUrl":"https://obop.example","clientId":"c"}');
  secrets.items.set("tokens", '{"accessToken":"a","refreshToken":"r","expiresAt":1}');
}

describe("which rules are for iOS", () => {
  test("the stale-file recovery and the sign-out of a new database are iOS only", () => {
    expect(policyFor("ios")).toEqual({
      recoverStaleFile: true,
      forgetSessionOnFreshDatabase: true,
    });
    expect(policyFor("android")).toEqual({
      recoverStaleFile: false,
      forgetSessionOnFreshDatabase: false,
    });
    expect(policyFor("web")).toEqual({
      recoverStaleFile: false,
      forgetSessionOnFreshDatabase: false,
    });
  });
});

describe("opening the database", () => {
  test("the first start makes a key, keeps it, and opens a new encrypted database", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    const opened = await openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, ANDROID);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(opened).toMatchObject({ fresh: true, recoveredStaleFile: false });
    expect(opened.cipherVersion).toBe("4.17.0 community");
    expect(file.keyOfFile).toBe(KEY);
    // The tables exist: the schema was migrated.
    const store = createSqlStore(opened.db);
    expect(await store.listTrips()).toEqual([]);
  });

  test("a later start reads the key that was kept and does not make another", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    await openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS);
    secrets.log.length = 0;
    const again = await openOutbox(fakePorts(file, secrets, { key: OTHER }), NAMES, IOS);
    expect(again).toMatchObject({ fresh: false, recoveredStaleFile: false });
    expect(secrets.log).toEqual([]);
    expect(file.deletes).toBe(0);
  });

  test("a plain SQLite is refused, the handle is closed, and nothing is replaced", async () => {
    const file = new FakeDatabaseFile();
    file.cipherVersion = null;
    const secrets = new FakeSecrets();
    await expect(openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS)).rejects.toThrow(
      "The database is not encrypted: this build has no SQLCipher",
    );
    expect(file.closes).toBe(file.opens);
    expect(file.deletes).toBe(0);
  });
});

describe("a file that no key fits", () => {
  test("on iOS, after a restore of a backup, the unreadable file is replaced and the driver is told", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    const secrets = new FakeSecrets(); // the Keychain item did not come with the backup
    const opened = await openOutbox(
      fakePorts(file, secrets, { key: KEY, now: Date.parse("2026-10-30T10:00:00Z") }),
      NAMES,
      IOS,
    );
    expect(opened).toMatchObject({ recoveredStaleFile: true, fresh: true });
    expect(file.deletes).toBe(1);
    // The failed handle was closed before the file was deleted (the fake refuses otherwise).
    expect(file.closes).toBe(1);
    // The new file is made with the new key, and the old content is gone.
    expect(file.keyOfFile).toBe(KEY);
    expect(secrets.items.get("db-key")).toBe(KEY);
    const store = createSqlStore(opened.db);
    expect(await store.getMeta(META_STALE_FILE_RECOVERED)).toBe("2026-10-30T10:00:00.000Z");
    expect(
      file
        .sqlite!.prepare("SELECT name FROM sqlite_master WHERE name = 'secret_of_the_old_phone'")
        .all(),
    ).toEqual([]);
  });

  test("on Android the file is left alone and the error is shown, as before", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    const secrets = new FakeSecrets();
    await expect(
      openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, ANDROID),
    ).rejects.toThrow(/file is not a database/);
    expect(file.deletes).toBe(0);
    expect(file.keyOfFile).toBe("f".repeat(64));
    expect(file.closes).toBe(file.opens);
  });

  test("a key that was read is never followed by a deletion, also on iOS", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile(OTHER);
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", KEY); // a key that was kept, and does not fit this file
    signedIn(secrets);
    await expect(
      openOutbox(fakePorts(file, secrets, { key: "c".repeat(64) }), NAMES, IOS),
    ).rejects.toThrow(/file is not a database/);
    expect(file.deletes).toBe(0);
    expect(file.keyOfFile).toBe(OTHER);
    expect(secrets.items.get("db-key")).toBe(KEY);
    // And the sign-in is still there: nothing was declared fresh.
    expect(secrets.items.has("tokens")).toBe(true);
  });

  test("an error that is not 'file is not a database' is not taken for a stale file", async () => {
    const file = new FakeDatabaseFile();
    file.failNextOpenWith = new Error("disk I/O error");
    const secrets = new FakeSecrets();
    await expect(openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS)).rejects.toThrow(
      "disk I/O error",
    );
    expect(file.deletes).toBe(0);
  });

  test("the words of SQLite for a key that does not fit", () => {
    expect(isNotADatabase(new Error("Error code 26: file is not a database"))).toBe(true);
    expect(isNotADatabase(new Error("file is encrypted or is not a database"))).toBe(true);
    expect(isNotADatabase(new Error("SQLITE_NOTADB"))).toBe(true);
    expect(isNotADatabase(new Error("Error code 266: something else"))).toBe(false);
    expect(isNotADatabase(new Error("database is locked"))).toBe(false);
    expect(isNotADatabase("file is not a database")).toBe(true);
  });
});

describe("a new database on iOS means a new installation", () => {
  test("the Keychain outlives the app: the sign-in it kept is dropped, the key is kept", async () => {
    const file = new FakeDatabaseFile(); // the app was deleted: no file
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", KEY);
    signedIn(secrets);
    const opened = await openOutbox(fakePorts(file, secrets, { key: OTHER }), NAMES, IOS);
    expect(opened).toMatchObject({ fresh: true, recoveredStaleFile: false });
    expect(secrets.items.has("server")).toBe(false);
    expect(secrets.items.has("tokens")).toBe(false);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(file.keyOfFile).toBe(KEY);
  });

  test("a database that already has its tables does not sign anybody out", async () => {
    const file = new FakeDatabaseFile();
    await file.makeFile(KEY);
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", KEY);
    signedIn(secrets);
    const opened = await openOutbox(fakePorts(file, secrets), NAMES, IOS);
    expect(opened.fresh).toBe(false);
    expect(secrets.items.has("tokens")).toBe(true);
  });

  test("on Android the sign-in is kept whatever the database looks like", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", KEY);
    signedIn(secrets);
    const opened = await openOutbox(fakePorts(file, secrets), NAMES, ANDROID);
    expect(opened.fresh).toBe(true);
    expect(secrets.items.has("server")).toBe(true);
    expect(secrets.items.has("tokens")).toBe(true);
  });
});

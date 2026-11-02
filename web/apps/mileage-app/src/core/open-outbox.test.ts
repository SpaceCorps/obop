import { describe, expect, test } from "vite-plus/test";
import { FakeDatabaseFile, FakeSecrets, fakePorts } from "../test-support/fake-file.ts";
import {
  META_STALE_FILE_RECOVERED,
  dismissStaleFileNote,
  isNotADatabase,
  openOutbox,
  policyFor,
  staleFileRecoveredAt,
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

  test("a stored value that is not a key is not used as one", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", "not-a-key");
    const opened = await openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, ANDROID);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(file.keyOfFile).toBe(KEY);
    expect(opened.fresh).toBe(true);
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

  test("the note about the replaced file stays until the driver has read it", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    const secrets = new FakeSecrets();
    const opened = await openOutbox(
      fakePorts(file, secrets, { key: KEY, now: Date.parse("2026-10-30T10:00:00Z") }),
      NAMES,
      IOS,
    );
    const store = createSqlStore(opened.db);
    expect(await staleFileRecoveredAt(store)).toBe("2026-10-30T10:00:00.000Z");
    // Still there at the next start of the app: the database is the same and so is the note.
    expect(await staleFileRecoveredAt(createSqlStore(opened.db))).toBe("2026-10-30T10:00:00.000Z");
    await dismissStaleFileNote(store);
    expect(await staleFileRecoveredAt(store)).toBeNull();
  });

  test("there is no note when nothing was replaced", async () => {
    const file = new FakeDatabaseFile();
    const opened = await openOutbox(fakePorts(file, new FakeSecrets(), { key: KEY }), NAMES, IOS);
    expect(await staleFileRecoveredAt(createSqlStore(opened.db))).toBeNull();
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

  test("a plain key is never followed by a deletion, also on iOS", async () => {
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

  test("a pending key is followed by a deletion when its file is not a database, also if an earlier start opened a file with it", async () => {
    // The first start made the file with KEY and was cut off before the key became plain. Before
    // the next start the file was replaced by another that KEY does not fit (a restore of a backup):
    // a pending key cannot tell this from a stale file, and the file is replaced.
    const file = new FakeDatabaseFile();
    await file.makeFile(KEY);
    expect(file.keyOfFile).toBe(KEY);
    file.plantStaleFile(OTHER);
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", `pending:${KEY}`);
    const opened = await openOutbox(fakePorts(file, secrets, { key: "c".repeat(64) }), NAMES, IOS);
    expect(opened).toMatchObject({ recoveredStaleFile: true, fresh: true });
    expect(file.deletes).toBe(1);
    expect(file.keyOfFile).toBe(KEY);
    expect(secrets.items.get("db-key")).toBe(KEY);
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

  test("a deletion that fails is put right by the next start (the key was stored before it)", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    file.failNextDeleteWith = new Error("The file is in use");
    const secrets = new FakeSecrets();
    const ports = fakePorts(file, secrets, { key: KEY });
    await expect(openOutbox(ports, NAMES, IOS)).rejects.toThrow("The file is in use");
    expect(file.deletes).toBe(0);
    // The key stays where the next start finds it, marked as one that opened nothing yet.
    expect(secrets.items.get("db-key")).toBe(`pending:${KEY}`);

    const opened = await openOutbox(ports, NAMES, IOS);
    expect(opened).toMatchObject({ recoveredStaleFile: true, fresh: true });
    expect(file.deletes).toBe(1);
    expect(file.keyOfFile).toBe(KEY);
    expect(secrets.items.get("db-key")).toBe(KEY);
    expect(await staleFileRecoveredAt(createSqlStore(opened.db))).not.toBeNull();
  });

  test("a start that was cut off after the key was stored is put right by the next one", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    // The first open of the first start fails with something that is not a wrong key (the process
    // died, the disk failed): the key is in the Keychain, the stale file is still there.
    file.failNextOpenWith = new Error("disk I/O error");
    const secrets = new FakeSecrets();
    const ports = fakePorts(file, secrets, { key: KEY });
    await expect(openOutbox(ports, NAMES, IOS)).rejects.toThrow("disk I/O error");
    expect(file.keyOfFile).toBe("f".repeat(64));
    expect(secrets.items.get("db-key")).toBe(`pending:${KEY}`);

    const opened = await openOutbox(ports, NAMES, IOS);
    expect(opened.recoveredStaleFile).toBe(true);
    expect(file.deletes).toBe(1);
    expect(file.keyOfFile).toBe(KEY);
    expect(secrets.items.get("db-key")).toBe(KEY);
  });

  test("a promotion that fails closes the open file, and the next start gets the file and the key right", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    const secrets = new FakeSecrets();
    const set = secrets.set;
    let failPromotion = true;
    secrets.set = async (name, value) => {
      // The promotion is the write of the plain key; the write of `pending:<key>` goes through.
      if (failPromotion && value === KEY) {
        failPromotion = false;
        throw new Error("The Keychain is not available");
      }
      return set(name, value);
    };
    const ports = fakePorts(file, secrets, { key: KEY });
    await expect(openOutbox(ports, NAMES, IOS)).rejects.toThrow("The Keychain is not available");
    // Every file that was opened was closed, also the one that opened fine (the stale one failed
    // and was closed before the deletion).
    expect(file.opens).toBe(2);
    expect(file.closes).toBe(2);
    // ... and each was closed once: a failure does not close a file a second time.
    expect(file.closeCalls).toBe(2);
    expect(file.keyOfFile).toBe(KEY);
    expect(secrets.items.get("db-key")).toBe(`pending:${KEY}`);

    // The retry opens the file again (a handle is not left behind by the first start), keeps what
    // is in it, and makes the key plain. The note of the replaced file was not written the second
    // time: the file is no longer stale.
    const opened = await openOutbox(ports, NAMES, IOS);
    expect(opened).toMatchObject({ recoveredStaleFile: false, fresh: false });
    expect(file.deletes).toBe(1);
    expect(file.closes).toBe(2);
    expect(file.opens).toBe(3);
    expect(secrets.items.get("db-key")).toBe(KEY);
  });

  test("a failure while the sign-in of a new database is dropped closes the file too", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    signedIn(secrets);
    secrets.delete = async () => {
      throw new Error("The Keychain is not available");
    };
    await expect(openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS)).rejects.toThrow(
      "The Keychain is not available",
    );
    expect(file.opens).toBe(1);
    expect(file.closes).toBe(1);
    expect(file.closeCalls).toBe(1);
  });

  test("a failure while the note of the replaced file is written closes the file too", async () => {
    const file = new FakeDatabaseFile();
    file.plantStaleFile();
    const secrets = new FakeSecrets();
    const ports = fakePorts(file, secrets, { key: KEY });
    const openFile = ports.openFile.bind(ports);
    ports.openFile = async () => {
      const handle = await openFile();
      return {
        ...handle,
        run: async (sql, params) => {
          if (params.includes(META_STALE_FILE_RECOVERED)) throw new Error("The disk is full");
          return handle.run(sql, params);
        },
      };
    };
    await expect(openOutbox(ports, NAMES, IOS)).rejects.toThrow("The disk is full");
    // The stale file (closed before it was replaced) and the new one, each closed once.
    expect(file.deletes).toBe(1);
    expect(file.opens).toBe(2);
    expect(file.closes).toBe(2);
    expect(file.closeCalls).toBe(2);
    // The key is still pending: the next start opens the new file and makes it plain.
    expect(secrets.items.get("db-key")).toBe(`pending:${KEY}`);
  });

  describe("a close that fails does not replace the error that is on its way", () => {
    const failure = (message: string) => new Error(message);

    for (const sync of [false, true]) {
      const how = sync ? "throws before it returns a promise" : "rejects";

      test(`a promotion that fails: the promotion's error is the one given when close ${how}`, async () => {
        const file = new FakeDatabaseFile();
        const secrets = new FakeSecrets();
        const set = secrets.set;
        secrets.set = async (name, value) => {
          if (value === KEY) throw new Error("The Keychain is not available");
          return set(name, value);
        };
        file.failNextCloseWith(failure("close failed"), { sync });
        await expect(
          openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS),
        ).rejects.toThrow("The Keychain is not available");
        // close was called once and failed; the handle stays open, and nothing closed it twice.
        expect(file.opens).toBe(1);
        expect(file.closeCalls).toBe(1);
      });

      test(`an open that fails: the error of the open is the one given when close ${how}`, async () => {
        const file = new FakeDatabaseFile();
        file.cipherVersion = null; // plain SQLite: openWithKey refuses it and closes the file
        const secrets = new FakeSecrets();
        file.failNextCloseWith(failure("close failed"), { sync });
        await expect(
          openOutbox(fakePorts(file, secrets, { key: KEY }), NAMES, IOS),
        ).rejects.toThrow("The database is not encrypted");
        expect(file.closeCalls).toBe(1);
      });
    }
  });

  test("a cut off start whose file did open is not mistaken for a stale one", async () => {
    // The key was made and the file was made with it, and the process died before the key became
    // plain: the next start opens the file with the pending key and keeps everything in it.
    const file = new FakeDatabaseFile();
    await file.makeFile(KEY);
    const secrets = new FakeSecrets();
    secrets.items.set("db-key", `pending:${KEY}`);
    const opened = await openOutbox(fakePorts(file, secrets, { key: OTHER }), NAMES, IOS);
    expect(opened).toMatchObject({ recoveredStaleFile: false, fresh: false });
    expect(file.deletes).toBe(0);
    expect(secrets.items.get("db-key")).toBe(KEY);
  });

  test("the key becomes plain only after a database was opened with it", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    const stored: (string | null)[] = [];
    const ports = fakePorts(file, secrets, { key: KEY });
    const openFile = ports.openFile.bind(ports);
    ports.openFile = async () => {
      stored.push(secrets.items.get("db-key") ?? null);
      return openFile();
    };
    await openOutbox(ports, NAMES, IOS);
    expect(stored).toEqual([`pending:${KEY}`]);
    expect(secrets.items.get("db-key")).toBe(KEY);
  });

  test("on Android the key is plain from the first moment, as before", async () => {
    const file = new FakeDatabaseFile();
    const secrets = new FakeSecrets();
    const stored: (string | null)[] = [];
    const ports = fakePorts(file, secrets, { key: KEY });
    const openFile = ports.openFile.bind(ports);
    ports.openFile = async () => {
      stored.push(secrets.items.get("db-key") ?? null);
      return openFile();
    };
    await openOutbox(ports, NAMES, ANDROID);
    expect(stored).toEqual([KEY]);
    expect(secrets.log.filter((line) => line === "set db-key")).toHaveLength(1);
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

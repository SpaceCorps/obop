import { beforeEach, describe, expect, test } from "vite-plus/test";
import { FakeDatabaseFile, FakeSecrets, fakePorts } from "../test-support/fake-file.ts";
import {
  TEST_FAILURES,
  canErase,
  databaseProblem,
  eraseDatabaseAndKey,
  openDatabase,
  testFailure,
} from "./open-db.ts";
import { SCHEMA_VERSION, SchemaError } from "./schema.ts";

const NAME = "org.obop.rutsa.database-key";
const NEW_KEY = "a".repeat(64);
const OLD_KEY = "b".repeat(64);

let file: FakeDatabaseFile;
let secrets: FakeSecrets;
const open = () => openDatabase(fakePorts(file, secrets, NEW_KEY), NAME);

beforeEach(() => {
  file = new FakeDatabaseFile();
  secrets = new FakeSecrets();
});

describe("opening the database", () => {
  test("the first start makes a key, stores it, and opens a new encrypted database", async () => {
    const opened = await open();
    expect(opened.cipherVersion).toBe("4.17.0 community");
    expect(opened.fresh).toBe(true);
    expect(opened.recoveredStaleFile).toBe(false);
    expect(secrets.items.get(NAME)).toBe(NEW_KEY);
    expect(file.keyOfFile).toBe(NEW_KEY);
    expect(await opened.db.one("PRAGMA user_version")).toEqual({ user_version: SCHEMA_VERSION });
    expect(await opened.db.one("PRAGMA foreign_keys")).toEqual({ foreign_keys: 1 });
  });

  test("the next start uses the stored key and finds the trips of the last one", async () => {
    const first = await open();
    await first.db.run(
      "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m) VALUES ('kept', 'ended', 'manual', 1, 2, 3)",
    );
    const next = await openDatabase(fakePorts(file, secrets, "c".repeat(64)), NAME);
    expect(next.fresh).toBe(false);
    expect(await next.db.one("SELECT id FROM trip")).toEqual({ id: "kept" });
    expect(secrets.items.get(NAME)).toBe(NEW_KEY);
  });

  test("a key without a file (the app was deleted, the Keychain kept the key) gets a new database", async () => {
    secrets.items.set(NAME, OLD_KEY);
    const opened = await open();
    expect(opened.fresh).toBe(true);
    expect(file.keyOfFile).toBe(OLD_KEY);
    expect(file.deletes).toBe(0);
  });

  test("a key that does not fit the file is an error, and the file is left alone", async () => {
    await file.makeFile(OLD_KEY);
    secrets.items.set(NAME, "d".repeat(64));
    await expect(open()).rejects.toThrow(/file is not a database/);
    expect(file.deletes).toBe(0);
    expect(file.keyOfFile).toBe(OLD_KEY);
    expect(secrets.items.get(NAME)).toBe("d".repeat(64));
    expect(file.openHandles).toBe(0);
  });

  test("a file whose key is gone (it came back with a backup) is replaced, and that is told", async () => {
    file.plantStaleFile();
    const opened = await open();
    expect(opened.recoveredStaleFile).toBe(true);
    expect(file.deletes).toBe(1);
    expect(file.keyOfFile).toBe(NEW_KEY);
    expect(secrets.items.get(NAME)).toBe(NEW_KEY);
    expect(await opened.db.one("SELECT COUNT(*) AS n FROM trip")).toEqual({ n: 0 });
  });

  test("a start cut off after storing a new key finds it pending and goes on with it", async () => {
    await file.makeFile(OLD_KEY);
    secrets.items.set(NAME, `pending:${OLD_KEY}`);
    const opened = await open();
    expect(opened.recoveredStaleFile).toBe(false);
    expect(file.deletes).toBe(0);
    expect(secrets.items.get(NAME)).toBe(OLD_KEY);
  });

  test("a pending key that does not fit the file is taken for a stale file", async () => {
    file.plantStaleFile();
    secrets.items.set(NAME, `pending:${OLD_KEY}`);
    const opened = await open();
    expect(opened.recoveredStaleFile).toBe(true);
    expect(file.keyOfFile).toBe(OLD_KEY);
    expect(secrets.items.get(NAME)).toBe(OLD_KEY);
  });

  test("a build without SQLCipher is refused, and nothing is written", async () => {
    file.cipherVersion = null;
    await expect(open()).rejects.toThrow(/not encrypted/);
    expect(file.keyOfFile).toBeNull();
    expect(file.openHandles).toBe(0);
  });

  test("a database from a newer version of the app is refused and closed", async () => {
    await file.makeFile(OLD_KEY);
    file.sqlite?.exec("PRAGMA user_version = 99");
    secrets.items.set(NAME, OLD_KEY);
    await expect(open()).rejects.toThrow(SchemaError);
    expect(file.openHandles).toBe(0);
    expect(file.deletes).toBe(0);
  });

  test("when the key cannot be made plain the file is closed and the error is told", async () => {
    // The first set stores the pending key, the second would make it plain.
    const store = secrets.set;
    let calls = 0;
    secrets.set = async (name, value) => {
      calls += 1;
      if (calls === 2) throw new Error("Keychain is not available");
      return store(name, value);
    };
    await expect(open()).rejects.toThrow("Keychain is not available");
    expect(file.openHandles).toBe(0);
    expect(secrets.items.get(NAME)).toBe(`pending:${NEW_KEY}`);
  });
});

describe("why the database could not be opened", () => {
  const failureOf = (promise: Promise<unknown>) =>
    promise.then(
      () => null,
      (e: unknown) => e,
    );

  test("each refusal of the open names its own reason", async () => {
    file.cipherVersion = null;
    expect(databaseProblem(await failureOf(open()))).toBe("no_cipher");

    file = new FakeDatabaseFile();
    await file.makeFile(OLD_KEY);
    secrets.items.set(NAME, "d".repeat(64));
    expect(databaseProblem(await failureOf(open()))).toBe("wrong_key");

    file = new FakeDatabaseFile();
    secrets = new FakeSecrets();
    await file.makeFile(NEW_KEY);
    secrets.items.set(NAME, NEW_KEY);
    file.sqlite?.exec("PRAGMA user_version = 99");
    const newer = await failureOf(open());
    expect(newer).toBeInstanceOf(SchemaError);
    expect(databaseProblem(newer)).toBe("newer");
  });

  test("what SQLite says about a wrong key, in its several wordings, is a wrong key", () => {
    for (const message of [
      "Call to function 'NativeDatabase.execAsync' has been rejected. Caused by: Error code 26: file is not a database",
      "file is encrypted or is not a database",
      "SQLITE_NOTADB",
    ]) {
      expect(databaseProblem(new Error(message)), message).toBe("wrong_key");
    }
  });

  test("anything else is other, also a SchemaError that is not about a newer version", () => {
    expect(databaseProblem(new Error("disk I/O error"))).toBe("other");
    expect(databaseProblem("a string")).toBe("other");
    expect(databaseProblem(new SchemaError("migration 2 has version 3"))).toBe("other");
  });
});

describe("erasing the trips when the database cannot be opened", () => {
  test("is offered only where it can help: a wrong key or an unknown cause", () => {
    expect(canErase("wrong_key")).toBe(true);
    expect(canErase("other")).toBe(true);
    // The file is sound; the newer version of the app opens it.
    expect(canErase("newer")).toBe(false);
    // A build without SQLCipher refuses a new file too.
    expect(canErase("no_cipher")).toBe(false);
  });

  test("the file goes first, then the key, and the next open starts empty with a new key", async () => {
    // A file with a trip, made by an earlier start; its key is lost (another one is in the secure
    // store), and the open fails.
    await file.makeFile(OLD_KEY);
    file.sqlite?.exec(
      "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m) VALUES ('gone', 'ended', 'manual', 1, 2, 3)",
    );
    secrets.items.set(NAME, "d".repeat(64));
    await expect(open()).rejects.toThrow(/file is not a database/);

    const order: string[] = [];
    await eraseDatabaseAndKey(
      {
        deleteFile: async () => {
          order.push("file");
          await file.deleteFile();
        },
        secrets: {
          delete: async (name) => {
            order.push(`key ${name}`);
            await secrets.delete(name);
          },
        },
      },
      NAME,
    );
    expect(order).toEqual(["file", `key ${NAME}`]);
    expect(secrets.items.has(NAME)).toBe(false);

    const next = await openDatabase(fakePorts(file, secrets, "e".repeat(64)), NAME);
    expect(next.fresh).toBe(true);
    expect(await next.db.all("SELECT id FROM trip")).toEqual([]);
    expect(secrets.items.get(NAME)).toBe("e".repeat(64));
  });

  test("a file that cannot be deleted keeps its key", async () => {
    secrets.items.set(NAME, OLD_KEY);
    await expect(
      eraseDatabaseAndKey(
        {
          deleteFile: async () => {
            throw new Error("Cannot delete a database that is open");
          },
          secrets,
        },
        NAME,
      ),
    ).rejects.toThrow("Cannot delete");
    expect(secrets.items.get(NAME)).toBe(OLD_KEY);
  });
});

describe("the failure a test build asks for (temporary)", () => {
  test("a normal build has none", () => {
    expect(testFailure(undefined)).toBeNull();
    expect(testFailure("")).toBeNull();
  });

  test("each named failure reads as its own reason on the screen", () => {
    for (const name of TEST_FAILURES) {
      expect(databaseProblem(testFailure(name)), name).toBe(name);
    }
  });

  test("a value that is not a failure fails too, and names itself", () => {
    const error = testFailure("wrong-key");
    expect(error?.message).toContain('"wrong-key"');
    expect(databaseProblem(error)).toBe("other");
  });
});

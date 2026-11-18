import { beforeEach, describe, expect, test } from "vite-plus/test";
import { FakeDatabaseFile, FakeSecrets, fakePorts } from "../test-support/fake-file.ts";
import { openDatabase } from "./open-db.ts";
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

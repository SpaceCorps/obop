import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { FakeDatabaseFile } from "../test-support/fake-file.ts";

// The open procedure itself is tested in `core/open-outbox.test.ts` with fake names. This file
// pins what only `native/database.ts` knows: the file name, the name of the key and the names of
// the session items that it hands to the procedure, and the names it passes to the system. They
// are what an installed app has, and a different name opens a different file or finds no key
// (on iOS that deletes the unsent trips, `core/open-outbox.ts`).

const keychain = vi.hoisted(() => {
  const AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY = 6;
  const items = new Map<string, string>();
  return {
    AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    items,
    sets: [] as { key: string; value: string }[],
    deletes: [] as string[],
    getItemAsync: async (key: string) => items.get(key) ?? null,
    setItemAsync: async (key: string, value: string) => {
      keychain.sets.push({ key, value });
      items.set(key, value);
    },
    deleteItemAsync: async (key: string) => {
      keychain.deletes.push(key);
      items.delete(key);
    },
  };
});
vi.mock("expo-secure-store", () => keychain);

const platform = vi.hoisted(() => ({ OS: "ios" as string }));
vi.mock("react-native", () => ({ Platform: platform }));

vi.mock("expo-crypto", () => ({
  getRandomBytesAsync: async (count: number) => new Uint8Array(count).fill(0xab),
}));

/** The file that the stand-in for expo-sqlite opens, and the names it was asked for. */
const sqlite = vi.hoisted(() => ({
  file: null as unknown as FakeDatabaseFile,
  opened: [] as string[],
  deleted: [] as string[],
}));
vi.mock("expo-sqlite", () => ({
  openDatabaseAsync: async (name: string) => {
    sqlite.opened.push(name);
    const handle = await sqlite.file.openFile();
    return {
      execAsync: (sql: string) => handle.exec(sql),
      runAsync: (sql: string, params: never[]) => handle.run(sql, params),
      getAllAsync: (sql: string, params: never[]) => handle.all(sql, params),
      closeAsync: () => handle.close(),
    };
  },
  deleteDatabaseAsync: async (name: string) => {
    sqlite.deleted.push(name);
    await sqlite.file.deleteFile();
  },
}));

const KEY = "ab".repeat(32);

async function load() {
  // The module keeps the one connection of the process: every test starts a new process.
  vi.resetModules();
  return import("./database.ts");
}

beforeEach(() => {
  keychain.items.clear();
  keychain.sets.length = 0;
  keychain.deletes.length = 0;
  sqlite.file = new FakeDatabaseFile();
  sqlite.opened.length = 0;
  sqlite.deleted.length = 0;
  platform.OS = "ios";
});

describe("the database file and its key", () => {
  test("the file is opened by the name an installed app has, once per process", async () => {
    const { openDatabase } = await load();
    const first = await openDatabase();
    const second = await openDatabase();
    expect(sqlite.opened).toEqual(["mileage.db"]);
    expect(second).toBe(first);
    expect(first.info.cipherVersion).toBe("4.17.0 community");
  });

  test("on iOS the key is kept as pending under its name first and plain after the open", async () => {
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.sets).toEqual([
      { key: "obop.mileage.database-key", value: `pending:${KEY}` },
      { key: "obop.mileage.database-key", value: KEY },
    ]);
    expect(sqlite.file.keyOfFile).toBe(KEY);
  });

  test("on Android the key is kept plain, under the same name", async () => {
    platform.OS = "android";
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.sets).toEqual([{ key: "obop.mileage.database-key", value: KEY }]);
  });

  test("a key that an earlier build kept is the key of the file", async () => {
    const stored = "cd".repeat(32);
    keychain.items.set("obop.mileage.database-key", stored);
    await sqlite.file.makeFile(stored);
    const { openDatabase } = await load();
    await openDatabase();
    expect(sqlite.file.keyOfFile).toBe(stored);
    expect(keychain.sets).toEqual([]);
    expect(sqlite.deleted).toEqual([]);
  });
});

describe("a new database on iOS forgets the sign-in by the names of its items", () => {
  test("the server item and the tokens item are deleted, the key item is not", async () => {
    keychain.items.set("obop.mileage.server", '{"baseUrl":"https://phone.example","clientId":"c"}');
    keychain.items.set(
      "obop.mileage.tokens",
      '{"accessToken":"a","refreshToken":"r","expiresAt":1}',
    );
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.deletes.sort()).toEqual(["obop.mileage.server", "obop.mileage.tokens"]);
    expect(keychain.items.has("obop.mileage.database-key")).toBe(true);
  });

  test("on Android nothing is deleted", async () => {
    platform.OS = "android";
    keychain.items.set("obop.mileage.server", '{"baseUrl":"https://phone.example","clientId":"c"}');
    keychain.items.set(
      "obop.mileage.tokens",
      '{"accessToken":"a","refreshToken":"r","expiresAt":1}',
    );
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.deletes).toEqual([]);
  });

  test("a database that already holds its tables signs nobody out", async () => {
    const stored = "cd".repeat(32);
    keychain.items.set("obop.mileage.database-key", stored);
    keychain.items.set(
      "obop.mileage.tokens",
      '{"accessToken":"a","refreshToken":"r","expiresAt":1}',
    );
    await sqlite.file.makeFile(stored);
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.deletes).toEqual([]);
  });
});

describe("the file of a restored backup", () => {
  test("is deleted by the name it was opened with, and the driver is told", async () => {
    sqlite.file.plantStaleFile();
    const { openDatabase } = await load();
    const { db } = await openDatabase();
    expect(sqlite.opened).toEqual(["mileage.db", "mileage.db"]);
    expect(sqlite.deleted).toEqual(["mileage.db"]);
    const note = await db.one<{ value: string }>("SELECT value FROM meta WHERE key = ?", [
      "stale_file_recovered_at",
    ]);
    expect(note?.value).toMatch(/^\d{4}-\d\d-\d\dT/);
  });

  test("is left alone on Android", async () => {
    platform.OS = "android";
    sqlite.file.plantStaleFile();
    const { openDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow(/file is not a database/);
    expect(sqlite.deleted).toEqual([]);
  });
});

describe("a failed open", () => {
  test("is not remembered: the next call opens again", async () => {
    sqlite.file.failNextOpenWith = new Error("disk I/O error");
    const { openDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow("disk I/O error");
    await expect(openDatabase()).resolves.toMatchObject({
      info: { cipherVersion: expect.any(String) },
    });
  });
});

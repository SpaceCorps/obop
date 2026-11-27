import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { FakeDatabaseFile } from "../test-support/fake-file.ts";

// The open procedure itself is tested in `core/open-db.test.ts` with fake names. This file pins what
// only `native/database.ts` and `native/store.ts` know: the name of the file and of its key that
// they hand to the procedure and to the system (an installed app has them, and another name opens
// another file or finds no key), that one connection serves the whole process, and that a failed
// open is tried again.
//
// After `mileage-app` (`src/native/database.test.ts`).

const keychain = vi.hoisted(() => {
  const items = new Map<string, string>();
  return {
    AFTER_FIRST_UNLOCK: 10,
    items,
    sets: [] as { key: string; value: string }[],
    getItemAsync: async (key: string) => items.get(key) ?? null,
    setItemAsync: async (key: string, value: string) => {
      keychain.sets.push({ key, value });
      items.set(key, value);
    },
    deleteItemAsync: async (key: string) => {
      keychain.deletes.push(key);
      items.delete(key);
    },
    deletes: [] as string[],
  };
});
vi.mock("expo-secure-store", () => keychain);

const crypto = vi.hoisted(() => ({ uuids: 0 }));
vi.mock("expo-crypto", () => ({
  getRandomBytesAsync: async (count: number) => new Uint8Array(count).fill(0xab),
  randomUUID: () => `uuid-${(crypto.uuids += 1)}`,
}));

/** The file that the stand-in for expo-sqlite opens, and the names it was asked for. */
const sqlite = vi.hoisted(() => ({
  file: null as unknown as FakeDatabaseFile,
  opened: [] as string[],
  deleted: [] as string[],
  failNextOpen: null as Error | null,
}));
vi.mock("expo-sqlite", () => ({
  openDatabaseAsync: async (name: string) => {
    sqlite.opened.push(name);
    if (sqlite.failNextOpen !== null) {
      const error = sqlite.failNextOpen;
      sqlite.failNextOpen = null;
      throw error;
    }
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

const FILE = "rutsa.db";
const KEY_NAME = "org.obop.rutsa.database-key";
const KEY = "ab".repeat(32);

/** The modules keep the one connection of the process: every test starts a new process. */
async function load() {
  vi.resetModules();
  return {
    ...(await import("./database.ts")),
    ...(await import("./store.ts")),
  };
}

beforeEach(() => {
  keychain.items.clear();
  keychain.sets.length = 0;
  keychain.deletes.length = 0;
  vi.unstubAllEnvs();
  crypto.uuids = 0;
  sqlite.file = new FakeDatabaseFile();
  sqlite.opened.length = 0;
  sqlite.deleted.length = 0;
  sqlite.failNextOpen = null;
});

describe("the database file and its key", () => {
  test("the file is opened by the name an installed app has, once per process", async () => {
    const { openDatabase } = await load();
    const first = await openDatabase();
    const second = await openDatabase();
    expect(sqlite.opened).toEqual([FILE]);
    expect(second).toBe(first);
    expect(first.cipherVersion).toBe("4.17.0 community");
  });

  test("a new key is kept as pending under its name first and plain after the open", async () => {
    const { openDatabase } = await load();
    await openDatabase();
    expect(keychain.sets).toEqual([
      { key: KEY_NAME, value: `pending:${KEY}` },
      { key: KEY_NAME, value: KEY },
    ]);
    expect(sqlite.file.keyOfFile).toBe(KEY);
  });

  test("a key that an earlier start kept is the key of the file", async () => {
    const stored = "cd".repeat(32);
    keychain.items.set(KEY_NAME, stored);
    await sqlite.file.makeFile(stored);
    const { openDatabase } = await load();
    const opened = await openDatabase();
    expect(sqlite.file.keyOfFile).toBe(stored);
    expect(keychain.sets).toEqual([]);
    expect(sqlite.deleted).toEqual([]);
    expect(opened.fresh).toBe(false);
  });

  test("a file that came back from a backup without its key is deleted by its name, and that is told", async () => {
    sqlite.file.plantStaleFile();
    const { databaseInfo } = await load();
    await expect(databaseInfo()).resolves.toEqual({
      cipherVersion: "4.17.0 community",
      recoveredStaleFile: true,
    });
    expect(sqlite.opened).toEqual([FILE, FILE]);
    expect(sqlite.deleted).toEqual([FILE]);
  });
});

describe("a failed open", () => {
  test("is not remembered: the next call opens again", async () => {
    sqlite.failNextOpen = new Error("disk I/O error");
    const { openDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow("disk I/O error");
    await expect(openDatabase()).resolves.toMatchObject({ cipherVersion: "4.17.0 community" });
    expect(sqlite.opened).toEqual([FILE, FILE]);
  });

  test("is not remembered by the store either", async () => {
    sqlite.failNextOpen = new Error("disk I/O error");
    const { getStore } = await load();
    await expect(getStore()).rejects.toThrow("disk I/O error");
    const store = await getStore();
    await expect(store.activeTrip()).resolves.toBeNull();
  });

  test("a key that does not fit the file is an error, and the file is left alone", async () => {
    await sqlite.file.makeFile("cd".repeat(32));
    keychain.items.set(KEY_NAME, "ef".repeat(32));
    const { getStore } = await load();
    await expect(getStore()).rejects.toThrow(/file is not a database/);
    expect(sqlite.deleted).toEqual([]);
    expect(sqlite.file.keyOfFile).toBe("cd".repeat(32));
  });
});

describe("erasing the trips after a failed open", () => {
  test("the file is deleted by its name, then the key by its name, and the next open starts empty", async () => {
    await sqlite.file.makeFile("cd".repeat(32));
    keychain.items.set(KEY_NAME, "ef".repeat(32));
    const { openDatabase, eraseDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow(/file is not a database/);

    await eraseDatabase();
    expect(sqlite.deleted).toEqual([FILE]);
    expect(keychain.deletes).toEqual([KEY_NAME]);

    const opened = await openDatabase();
    expect(opened.fresh).toBe(true);
    expect(sqlite.file.keyOfFile).toBe(KEY);
    expect(keychain.items.get(KEY_NAME)).toBe(KEY);
  });

  test("a file that cannot be deleted keeps its key", async () => {
    await sqlite.file.makeFile("cd".repeat(32));
    keychain.items.set(KEY_NAME, "cd".repeat(32));
    const { eraseDatabase } = await load();
    // A handle that is still open: expo-sqlite refuses to delete the file.
    await sqlite.file.openFile();
    await expect(eraseDatabase()).rejects.toThrow(/open/);
    expect(keychain.deletes).toEqual([]);
    expect(keychain.items.get(KEY_NAME)).toBe("cd".repeat(32));
  });
});

describe("a test build that pretends the open fails (temporary)", () => {
  test("every open fails with the reason asked for, and nothing is touched", async () => {
    vi.stubEnv("EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE", "wrong_key");
    const { openDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow(/file is not a database/);
    await expect(openDatabase()).rejects.toThrow(/file is not a database/);
    expect(sqlite.opened).toEqual([]);
    expect(keychain.sets).toEqual([]);
  });

  test("after the trips are erased the real database opens", async () => {
    vi.stubEnv("EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE", "newer");
    const { openDatabase, eraseDatabase } = await load();
    await expect(openDatabase()).rejects.toThrow(/newer version of Rutsa/);
    await eraseDatabase();
    await expect(openDatabase()).resolves.toMatchObject({ fresh: true });
    expect(sqlite.opened).toEqual([FILE]);
  });

  test("a normal build pretends nothing", async () => {
    const { openDatabase } = await load();
    await expect(openDatabase()).resolves.toMatchObject({ fresh: true });
  });
});

describe("the store", () => {
  test("the screens and the location task share one store over one connection", async () => {
    const { getStore, databaseInfo } = await load();
    const [a, b] = await Promise.all([getStore(), getStore()]);
    expect(b).toBe(a);
    await databaseInfo();
    expect(sqlite.opened).toEqual([FILE]);
  });

  test("a trip gets a random UUID as its id, and is in the file", async () => {
    const { getStore } = await load();
    const store = await getStore();
    const trip = await store.startTrip(1_000);
    expect(trip.id).toBe("uuid-1");
    await expect(store.activeTrip()).resolves.toMatchObject({ id: "uuid-1", state: "recording" });
  });
});

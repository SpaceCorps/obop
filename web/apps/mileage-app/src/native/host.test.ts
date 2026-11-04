import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { defineIdentity } from "../core/host-config.ts";
import { defineStartup } from "../core/startup-config.ts";

// The host is given the configuration of the app once, at the top of the entry, and everything
// that needs it asks for it when it runs. The background task is imported before the rest of the
// app and is called by the system with no screen and no runtime object, so nothing in what it
// imports may read the configuration while it is being imported.

const keychainItems = vi.hoisted(() => new Map<string, string>());
vi.mock("expo-secure-store", () => ({
  AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY: 6,
  getItemAsync: async (key: string) => keychainItems.get(key) ?? null,
  setItemAsync: async (key: string, value: string) => {
    keychainItems.set(key, value);
  },
  deleteItemAsync: async (key: string) => {
    keychainItems.delete(key);
  },
}));
vi.mock("expo-crypto", () => ({
  getRandomBytesAsync: async (n: number) => new Uint8Array(n),
  digest: async () => new ArrayBuffer(32),
  CryptoDigestAlgorithm: { SHA256: "SHA-256" },
}));
vi.mock("expo-sqlite", () => ({
  openDatabaseAsync: async () => {
    throw new Error("no database in this test");
  },
  deleteDatabaseAsync: async () => undefined,
}));
vi.mock("expo-task-manager", () => ({ defineTask: () => undefined }));
vi.mock("expo-location", () => ({
  Accuracy: { High: 4 },
  startLocationUpdatesAsync: async () => undefined,
  stopLocationUpdatesAsync: async () => undefined,
  hasStartedLocationUpdatesAsync: async () => false,
}));
vi.mock("expo-constants", () => ({ default: { expoConfig: { extra: {} } } }));
vi.mock("react-native", () => ({ Platform: { OS: "ios", Version: 27 } }));

const other = defineStartup({
  identity: defineIdentity({
    applicationId: "se.example.app",
    clientName: "Example",
    version: "2.5.1",
    locales: ["sv", "en"],
    storage: {
      databaseFile: "example.db",
      databaseKey: "example.database-key",
      server: "example.server",
      tokens: "example.tokens",
    },
  }),
  groups: ["example.user"],
});

beforeEach(() => {
  vi.resetModules();
  keychainItems.clear();
});

describe("the holder of the configuration", () => {
  test("says what to do when nothing has started the host", async () => {
    const { hostConfig } = await import("./host.ts");
    expect(() => hostConfig()).toThrow(/startHost/);
  });

  test("gives back what it was started with, also when it is started again with the same object", async () => {
    const { hostConfig, startHost } = await import("./host.ts");
    startHost(other);
    startHost(other);
    expect(hostConfig()).toBe(other);
  });

  test("refuses to be started with another configuration", async () => {
    const { startHost } = await import("./host.ts");
    const { startup } = await import("../../config/startup.ts");
    startHost(startup);
    expect(() => startHost(other)).toThrow(/another configuration/);
  });
});

describe("the code the background task imports", () => {
  test("can be imported before the host is started, and asks for the configuration when it is used", async () => {
    // The task, the store, the database, the Keychain and the platform dependencies: the whole
    // closure of `task.ts`, with nothing started.
    await import("./task.ts");
    const { openDatabase } = await import("./database.ts");
    const { appVersion } = await import("./platform-deps.ts");
    const { getStore } = await import("./store.ts");
    await expect(openDatabase()).rejects.toThrow(/startHost/);
    await expect(getStore()).rejects.toThrow(/startHost/);
    expect(() => appVersion()).toThrow(/startHost/);
  });

  test("reads what the app gave it once it is started", async () => {
    const { startHost } = await import("./host.ts");
    const { appVersion } = await import("./platform-deps.ts");
    startHost(other);
    expect(appVersion()).toBe("2.5.1");
  });

  test("a failed attempt before the start is not remembered", async () => {
    const { startHost } = await import("./host.ts");
    const { openDatabase } = await import("./database.ts");
    await expect(openDatabase()).rejects.toThrow(/startHost/);
    startHost(other);
    // The next call gets as far as the file (which this test does not have) with the names of the app.
    await expect(openDatabase()).rejects.toThrow("no database in this test");
  });
});

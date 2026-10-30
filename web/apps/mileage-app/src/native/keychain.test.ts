import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// A stand-in for expo-secure-store that keeps the one rule of the iOS Keychain that matters here:
// an item stored with no class (or `WHEN_UNLOCKED`) cannot be read while the phone is locked; an
// item stored `AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY` can.
const fake = vi.hoisted(() => {
  const WHEN_UNLOCKED = 5;
  const AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY = 6;
  const items = new Map<string, { value: string; kind: number }>();
  const state = { locked: false };
  return {
    WHEN_UNLOCKED,
    AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    items,
    state,
    sets: [] as { key: string; options: { keychainAccessible?: number } | undefined }[],
    getItemAsync: async (key: string) => {
      const item = items.get(key);
      if (item === undefined) return null;
      if (state.locked && item.kind === WHEN_UNLOCKED) {
        throw new Error("Could not get the value: User interaction is not allowed.");
      }
      return item.value;
    },
    setItemAsync: async (key: string, value: string, options?: { keychainAccessible?: number }) => {
      fake.sets.push({ key, options });
      items.set(key, { value, kind: options?.keychainAccessible ?? WHEN_UNLOCKED });
    },
    deleteItemAsync: async (key: string) => {
      items.delete(key);
    },
  };
});
vi.mock("expo-secure-store", () => fake);

const { keychain } = await import("./keychain.ts");
const { secureStorage } = await import("./secure-storage.ts");

beforeEach(() => {
  fake.items.clear();
  fake.sets.length = 0;
  fake.state.locked = false;
});

describe("what the app keeps in the Keychain", () => {
  test("every value is stored after first unlock, this device only", async () => {
    await keychain.set("a", "1");
    expect(fake.sets).toEqual([
      { key: "a", options: { keychainAccessible: fake.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY } },
    ]);
  });

  test("the session is stored the same way", async () => {
    await secureStorage.saveConfig({ baseUrl: "https://obop.example", clientId: "c1" });
    await secureStorage.saveTokens({
      accessToken: "a",
      refreshToken: "r",
      expiresAt: 1,
      deviceId: null,
    } as never);
    expect(fake.sets.map((s) => s.options?.keychainAccessible)).toEqual([
      fake.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
      fake.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    ]);
  });

  test("with the phone locked the location task can still read what it needs", async () => {
    await keychain.set("obop.mileage.database-key", "k".repeat(64));
    await secureStorage.saveTokens({
      accessToken: "a",
      refreshToken: "r",
      expiresAt: 1,
      deviceId: null,
    } as never);
    fake.state.locked = true;
    await expect(keychain.get("obop.mileage.database-key")).resolves.toBe("k".repeat(64));
    await expect(secureStorage.loadTokens()).resolves.toMatchObject({ refreshToken: "r" });
  });

  test("the fake does what the default class of the library does: an item with no class is not readable while locked", async () => {
    const SecureStore = await import("expo-secure-store");
    await SecureStore.setItemAsync("plain", "x");
    fake.state.locked = true;
    await expect(SecureStore.getItemAsync("plain")).rejects.toThrow(/not allowed/);
  });

  test("no other file of the app imports expo-secure-store or writes to it", () => {
    const src = join(dirname(fileURLToPath(import.meta.url)), "..");
    const offenders: string[] = [];
    for (const name of readdirSync(src, { recursive: true }) as string[]) {
      const path = name.replaceAll("\\", "/");
      if (!/\.tsx?$/.test(path) || path.endsWith(".test.ts") || path === "native/keychain.ts") {
        continue;
      }
      // A use is an import or a call; the words may stand in a comment.
      if (/from "expo-secure-store"|setItemAsync\(/.test(readFileSync(join(src, name), "utf8"))) {
        offenders.push(path);
      }
    }
    expect(offenders).toEqual([]);
  });
});

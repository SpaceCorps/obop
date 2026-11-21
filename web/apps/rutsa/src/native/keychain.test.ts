import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { appDir } from "../test-support/imports.ts";

// A stand-in for expo-secure-store that keeps the one rule of the iOS Keychain that matters here:
// an item stored with no class (the library's default, `WHEN_UNLOCKED`) cannot be read while the
// phone is locked; an item stored `AFTER_FIRST_UNLOCK` (with or without "this device only") can.
// The numbers of the classes are the fake's own; only which one is passed matters.
//
// After `mileage-app` (`src/native/keychain.test.ts`).
const fake = vi.hoisted(() => {
  const AFTER_FIRST_UNLOCK = 10;
  const AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY = 11;
  const WHEN_UNLOCKED = 12;
  const items = new Map<string, { value: string; kind: number }>();
  const state = { locked: false };
  const sets: { key: string; options: { keychainAccessible?: number } | undefined }[] = [];
  return {
    AFTER_FIRST_UNLOCK,
    AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    WHEN_UNLOCKED,
    items,
    state,
    sets,
    getItemAsync: async (key: string) => {
      const item = items.get(key);
      if (item === undefined) return null;
      if (state.locked && item.kind === WHEN_UNLOCKED) {
        throw new Error("Could not get the value: User interaction is not allowed.");
      }
      return item.value;
    },
    setItemAsync: async (key: string, value: string, options?: { keychainAccessible?: number }) => {
      sets.push({ key, options });
      items.set(key, { value, kind: options?.keychainAccessible ?? WHEN_UNLOCKED });
    },
  };
});
vi.mock("expo-secure-store", () => fake);

const { keychain } = await import("./keychain.ts");
const { identity } = await import("../../config/identity.ts");

beforeEach(() => {
  fake.items.clear();
  fake.sets.length = 0;
  fake.state.locked = false;
});

describe("what the app keeps in the Keychain", () => {
  test("a value is stored after first unlock, and not this device only", async () => {
    await keychain.set("a", "1");
    // Not `AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY` as in mileage-app: the phone holds the only copy of
    // the trips, and the key has to go with the database into an encrypted backup or to a new
    // iPhone (`keychain.ts`).
    expect(fake.sets).toEqual([
      { key: "a", options: { keychainAccessible: fake.AFTER_FIRST_UNLOCK } },
    ]);
  });

  test("with the phone locked the location task can still read the key of the database", async () => {
    await keychain.set(identity.storage.databaseKey, "k".repeat(64));
    fake.state.locked = true;
    await expect(keychain.get(identity.storage.databaseKey)).resolves.toBe("k".repeat(64));
  });

  test("a name that was never stored reads as nothing", async () => {
    await expect(keychain.get("nothing")).resolves.toBeNull();
  });

  test("the fake does what the default class of the library does: an item with no class is not readable while locked", async () => {
    const SecureStore = await import("expo-secure-store");
    await SecureStore.setItemAsync("plain", "x");
    fake.state.locked = true;
    await expect(SecureStore.getItemAsync("plain")).rejects.toThrow(/not allowed/);
  });

  test("no other file of the app imports expo-secure-store or writes to it", () => {
    const sources = [
      "App.tsx",
      "index.ts",
      ...(readdirSync(join(appDir, "src"), { recursive: true }) as string[]).map(
        (name) => `src/${name.replaceAll("\\", "/")}`,
      ),
    ];
    const offenders = sources.filter(
      (path) =>
        /\.tsx?$/.test(path) &&
        !path.endsWith(".test.ts") &&
        path !== "src/native/keychain.ts" &&
        // A use is an import or a call; the words may stand in a comment.
        /from "expo-secure-store"|setItemAsync\(/.test(readFileSync(join(appDir, path), "utf8")),
    );
    expect(offenders).toEqual([]);
  });
});

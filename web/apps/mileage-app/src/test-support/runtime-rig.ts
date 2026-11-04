import type { SqlDb } from "../core/sql.ts";
import { createSqlStore, migrate } from "../core/sql-store.ts";
import { memoryDb } from "./memory-db.ts";

/**
 * The stand-ins under the runtime of the app (`native/runtime.ts`): the Keychain, the database, the
 * network and the platform. A test file mocks `./keychain.ts`, `./database.ts` and
 * `./platform-deps.ts` with the functions at the end of this file and imports the runtime after
 * `vi.resetModules()`, so that each test starts a new process. The names of the Keychain items are
 * literals on purpose: they are what a phone that has the app already holds.
 */

export const BASE = "https://phone.example";
export const SERVER_ITEM = "obop.mileage.server";
export const TOKENS_ITEM = "obop.mileage.tokens";

export interface Call {
  method: string;
  path: string;
  body: string;
}

interface World {
  keychain: Map<string, string>;
  calls: Call[];
  db: SqlDb | null;
  failOpenOnce: boolean;
  offline: boolean;
  revokeStatus: number;
  holdRevoke: boolean;
  releaseRevoke: (() => void) | null;
  deviceRevokeStatus: number;
  keychainDeleteFails: boolean;
}

// One object for the whole test file: `vi.resetModules()` makes a new copy of this module for each
// test, but a `vi.mock` factory keeps the copy it was made with, so the state is not kept in the module.
const shared = globalThis as { __mileageRigWorld?: World };

export const world: World = (shared.__mileageRigWorld ??= {
  keychain: new Map<string, string>(),
  calls: [] as Call[],
  db: null as SqlDb | null,
  /** The next open of the database fails, once. */
  failOpenOnce: false,
  /** Every call that reaches the network fails as if there were no connection. */
  offline: false,
  /** The answer of `POST /oauth/revoke`; held back until `releaseRevoke` when `holdRevoke` is set. */
  revokeStatus: 200,
  holdRevoke: false,
  releaseRevoke: null as (() => void) | null,
  /** The answer of `POST /api/device/revoke`. */
  deviceRevokeStatus: 200,
  /** The delete of a Keychain item fails (the Keystore refuses the write of a sign-out). */
  keychainDeleteFails: false,
});

export function reset(): void {
  world.keychain.clear();
  world.calls.length = 0;
  world.db = null;
  world.failOpenOnce = false;
  world.offline = false;
  world.revokeStatus = 200;
  world.holdRevoke = false;
  world.releaseRevoke = null;
  world.deviceRevokeStatus = 200;
  world.keychainDeleteFails = false;
}

export async function newDb(): Promise<SqlDb> {
  const db = memoryDb();
  await migrate(db);
  return db;
}

/** The database of this process, made before the runtime asks for it so that a test can seed it. */
export async function database(): Promise<SqlDb> {
  world.db ??= await newDb();
  return world.db;
}

export function signedInPhone(
  options: { user?: string; baseUrl?: string; expiresAt?: number } = {},
): void {
  const { user = "dave", baseUrl = BASE, expiresAt = Date.now() + 3_600_000 } = options;
  world.keychain.set(
    SERVER_ITEM,
    JSON.stringify({ baseUrl, clientId: "client-1", user, usedAt: Date.now() }),
  );
  world.keychain.set(
    TOKENS_ITEM,
    JSON.stringify({
      accessToken: "acc-1",
      refreshToken: "ref-1",
      expiresAt,
      deviceId: "dev-1",
      scope: ["mileage.driver"],
    }),
  );
}

export async function setOwner(user: string, origin = BASE): Promise<void> {
  await createSqlStore(await database()).setMeta("owner", JSON.stringify({ origin, user }));
}

/** A trip with `points` stored points. `stopped` trips are finished on the phone but not sent. */
export async function seedTrip(id: string, points: number, stopped: boolean): Promise<void> {
  const store = createSqlStore(await database());
  const t0 = Date.parse("2026-10-15T09:00:00Z");
  await store.startTrip({
    clientTripId: id,
    purpose: "business",
    startedAt: new Date(t0).toISOString(),
    nowMs: t0,
  });
  await store.appendFixes(
    id,
    Array.from({ length: points }, (_, i) => ({
      t: t0 + (i + 1) * 5000,
      lat: 52 + i * 0.0006,
      lon: 13,
      accuracyM: 5,
      speedMps: 3,
    })),
  );
  if (stopped) await store.stopTrip(id, new Date(t0 + (points + 1) * 5000).toISOString());
}

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** The network: records every call; answers what the sign-out and the device calls need. */
export async function network(input: string | Request, init?: RequestInit): Promise<Response> {
  const url = typeof input === "string" ? input : input.url;
  const method = (typeof input === "string" ? init?.method : input.method) ?? "GET";
  let body = "";
  if (typeof input !== "string") body = await input.clone().text();
  else if (typeof init?.body === "string") body = init.body;
  const path = new URL(url).pathname;
  world.calls.push({ method, path, body });
  if (world.offline) throw new TypeError("Network request failed");
  if (path === "/oauth/revoke") {
    if (world.holdRevoke) {
      await new Promise<void>((resolve) => {
        world.releaseRevoke = resolve;
      });
    }
    return json(world.revokeStatus === 200 ? {} : { error: "server_error" }, world.revokeStatus);
  }
  if (path === "/api/device/revoke") {
    return json(
      world.deviceRevokeStatus === 200 ? { value: {} } : { error: "server_error" },
      world.deviceRevokeStatus,
    );
  }
  return json({ error: "not_found" }, 404);
}

export const callsTo = (path: string): Call[] => world.calls.filter((c) => c.path === path);

// The factories for `vi.mock`.

export function keychainMock() {
  return {
    keychain: {
      get: async (name: string) => world.keychain.get(name) ?? null,
      set: async (name: string, value: string) => {
        world.keychain.set(name, value);
      },
      delete: async (name: string) => {
        if (world.keychainDeleteFails) throw new Error("The Keystore refused the write");
        world.keychain.delete(name);
      },
    },
  };
}

export function databaseMock() {
  return {
    openDatabase: async () => {
      if (world.failOpenOnce) {
        world.failOpenOnce = false;
        throw new Error("disk I/O error");
      }
      return { db: await database(), info: { cipherVersion: "test" } };
    },
  };
}

export async function platformDepsMock() {
  const { createHash } = await import("node:crypto");
  return {
    oauthDeps: {
      sha256: async (text: string) => new Uint8Array(createHash("sha256").update(text).digest()),
      randomBytes: async (count: number) => new Uint8Array(count).fill(5),
      fetch: network,
    },
    deviceInfo: () => ({ name: "Test phone", platform: "ios", appVersion: "0.1.0" }),
    appVersion: () => "0.1.0",
  };
}

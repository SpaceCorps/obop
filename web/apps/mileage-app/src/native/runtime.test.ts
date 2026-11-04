import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { Runtime } from "./runtime.ts";

// The runtime object that every screen uses, built from a restored session and an outbox. These
// tests describe what it does today, with the real session, the real outbox over SQLite, the real
// sync controller and the real engine; only the Keychain, the network, the browser and the platform
// are stand-ins (`test-support/runtime-rig.ts`). They were written before the runtime was touched,
// and have to say the same after.

vi.mock("expo-web-browser", () => ({}));
vi.mock("expo-constants", () => ({ default: { expoConfig: { extra: {} } } }));
vi.mock("./keychain.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).keychainMock(),
);
vi.mock("./database.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).databaseMock(),
);
vi.mock("./platform-deps.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).platformDepsMock(),
);

type Rig = typeof import("../test-support/runtime-rig.ts");
let rig: Rig;
let emits: number;
let off: () => void;

/** A new process: nothing is cached, the Keychain and the database are empty. */
async function newProcess() {
  vi.resetModules();
  rig = await import("../test-support/runtime-rig.ts");
  rig.reset();
  rig.boot();
  vi.stubGlobal("fetch", rig.network);
  await rig.database();
  const bus = await import("./bus.ts");
  emits = 0;
  off?.();
  off = bus.onChange(() => {
    emits += 1;
  });
}

async function start(): Promise<Runtime> {
  const { getRuntime } = await import("./runtime.ts");
  return getRuntime();
}

/** Lets the pass of the sync controller that a kick starts run to its end. */
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 20));

beforeEach(newProcess);

describe("a start without a sign-in", () => {
  test("is signed out; the client refuses; nothing is sent and nothing is announced", async () => {
    const rt = await start();
    await settle();
    expect(rt.session.status()).toBe("signed_out");
    expect(rt.binding()).toEqual({ kind: "own" });
    expect(() => rt.client()).toThrowError(
      expect.objectContaining({ name: "ObopApiError", status: 401, code: "not_signed_in" }),
    );
    expect(rt.sync.status().report).toBeNull();
    expect(rig.world.calls).toEqual([]);
    // Restoring the session happens before the runtime listens, so it announces nothing.
    expect(emits).toBe(0);
  });

  test("a stored session at an address the build may not use is dropped, and nothing is announced", async () => {
    rig.signedInPhone({ baseUrl: "http://192.168.1.5:18080" });
    const rt = await start();
    expect(rt.session.status()).toBe("signed_out");
    expect(rig.world.keychain.has(rig.TOKENS_ITEM)).toBe(false);
    expect(rig.world.keychain.has(rig.SERVER_ITEM)).toBe(false);
    expect(emits).toBe(0);
  });
});

describe("a start with the session of the person who owns the outbox", () => {
  beforeEach(async () => {
    rig.signedInPhone({ user: "dave" });
    await rig.setOwner("dave");
  });

  test("is signed in, the outbox is its own, the client talks to the stored address, and a pass starts", async () => {
    const rt = await start();
    expect(rt.session.status()).toBe("signed_in");
    expect(rt.binding()).toEqual({ kind: "own" });
    const client = rt.client();
    expect(rt.client()).toBe(client); // cached for the same address
    await settle();
    // The first pass ran (an empty outbox has nothing to send).
    expect(rt.sync.status().report).not.toBeNull();
    expect(emits).toBeGreaterThan(0);
  });

  test("asking for the runtime again gives the same object", async () => {
    const first = await start();
    const second = await start();
    expect(second).toBe(first);
  });

  test("bindIdentity announces once and does not start a pass", async () => {
    const rt = await start();
    await settle();
    const before = { emits, status: rt.sync.status() };
    await expect(rt.bindIdentity({ origin: rig.BASE, user: "dave" })).resolves.toEqual({
      kind: "own",
    });
    expect(emits - before.emits).toBe(1);
    expect(rt.sync.status()).toBe(before.status);
  });
});

describe("a start with the session of another person than the owner of the outbox", () => {
  beforeEach(async () => {
    rig.signedInPhone({ user: "rita" });
    await rig.setOwner("dave");
    await rig.seedTrip("c1", 3, true);
  });

  test("the outbox is foreign: the client refuses and nothing is sent", async () => {
    const rt = await start();
    await settle();
    expect(rt.session.status()).toBe("signed_in");
    expect(rt.binding()).toEqual({ kind: "foreign", unsent: { trips: 1, points: 3 } });
    expect(() => rt.client()).toThrowError(expect.objectContaining({ status: 401 }));
    expect(rt.sync.status().report).toBeNull();
    expect(rig.world.calls).toEqual([]);
  });

  test("bindIdentity keeps it foreign while trips are unsent, and nothing is deleted", async () => {
    const rt = await start();
    const binding = await rt.bindIdentity({ origin: rig.BASE, user: "rita" });
    expect(binding).toEqual({ kind: "foreign", unsent: { trips: 1, points: 3 } });
    expect((await rt.store.listTrips()).map((t) => t.clientTripId)).toEqual(["c1"]);
  });

  test("taking the outbox over clears it, makes the new person the owner, announces and starts a pass", async () => {
    const rt = await start();
    await settle();
    const before = emits;
    await rt.takeOverOutbox();
    expect(rt.binding()).toEqual({ kind: "own" });
    expect(await rt.store.listTrips()).toEqual([]);
    expect(await rt.store.getMeta("owner")).toBe(
      JSON.stringify({ origin: rig.BASE, user: "rita" }),
    );
    expect(() => rt.client()).not.toThrow();
    await settle();
    // One announcement of the binding, and the pass of the sync controller after it.
    expect(emits - before).toBeGreaterThan(1);
    expect(rt.sync.status().report).not.toBeNull();
  });
});

describe("a new sign-in finishes as someone with an empty outbox", () => {
  test("bindIdentity adopts the outbox and announces once", async () => {
    const rt = await start();
    const before = emits;
    await expect(rt.bindIdentity({ origin: rig.BASE, user: "dave" })).resolves.toEqual({
      kind: "own",
    });
    expect(emits - before).toBe(1);
    expect(await rt.store.getMeta("owner")).toBe(
      JSON.stringify({ origin: rig.BASE, user: "dave" }),
    );
  });
});

describe("telling the instance that the sign-in is over", () => {
  test("without a token there is nothing to tell, and that counts as told", async () => {
    const rt = await start();
    await expect(rt.revokeSignIn()).resolves.toBe(true);
    expect(rig.world.calls).toEqual([]);
  });

  test("with a token it is sent with the client, and an answer means told", async () => {
    rig.signedInPhone();
    const rt = await start();
    await expect(rt.revokeSignIn()).resolves.toBe(true);
    const [call] = rig.callsTo("/oauth/revoke");
    expect(Object.fromEntries(new URLSearchParams(call!.body))).toMatchObject({
      token: "ref-1",
      client_id: "client-1",
    });
  });

  test("an instance that cannot be reached means not told", async () => {
    rig.signedInPhone();
    const rt = await start();
    rig.world.offline = true;
    await expect(rt.revokeSignIn()).resolves.toBe(false);
  });

  test("an instance that refuses means not told", async () => {
    rig.signedInPhone();
    const rt = await start();
    rig.world.revokeStatus = 500;
    await expect(rt.revokeSignIn()).resolves.toBe(false);
  });
});

describe("the flag that the last sign-out could not tell the instance", () => {
  test("is announced once per change and not when it stays", async () => {
    const rt = await start();
    expect(rt.signOutUntold()).toBe(false);
    rt.setSignOutUntold(false);
    expect(emits).toBe(0);
    rt.setSignOutUntold(true);
    rt.setSignOutUntold(true);
    expect(rt.signOutUntold()).toBe(true);
    expect(emits).toBe(1);
    rt.setSignOutUntold(false);
    expect(emits).toBe(2);
  });
});

describe("a database that cannot be opened", () => {
  test("rejects, is not remembered, and the next call builds the runtime", async () => {
    rig.world.failOpenOnce = true;
    const { getRuntime } = await import("./runtime.ts");
    await expect(getRuntime()).rejects.toThrow("disk I/O error");
    const rt = await getRuntime();
    expect(rt.session.status()).toBe("signed_out");
  });
});

import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { Runtime } from "./runtime.ts";

// What a sign-out does, and in which order (`signOut` and `leaveSession` in `actions.ts`, the
// runtime, the session and the outbox are the real ones; the Keychain, the network, the recorder
// and the platform are stand-ins). The order is the contract: the request to the instance is
// started first because it needs the refresh token that the sign-out then forgets; the phone ends
// its own side without waiting for the answer; the answer is awaited last. A trip that is not
// completely on the server is deleted only when the person said so, and never when the tokens could
// not be removed.

const events = vi.hoisted(() => [] as string[]);

vi.mock("expo-web-browser", () => ({}));
vi.mock("expo-constants", () => ({ default: { expoConfig: { extra: {} } } }));
vi.mock("expo-crypto", () => ({ getRandomBytesAsync: async (n: number) => new Uint8Array(n) }));
vi.mock("./keychain.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).keychainMock(),
);
vi.mock("./database.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).databaseMock(),
);
vi.mock("./platform-deps.ts", async () =>
  (await import("../test-support/runtime-rig.ts")).platformDepsMock(),
);
vi.mock("./recorder.ts", () => ({
  startRecording: async () => undefined,
  ensureRecording: async () => "ready",
  stopRecording: async () => {
    events.push("recorder.stop");
  },
}));
vi.mock("./permissions.ts", () => ({ locationReadiness: async () => "ready" }));

type Rig = typeof import("../test-support/runtime-rig.ts");
let rig: Rig;

async function newProcess() {
  vi.resetModules();
  events.length = 0;
  rig = await import("../test-support/runtime-rig.ts");
  rig.reset();
  vi.stubGlobal("fetch", rig.network);
  await rig.database();
  const bus = await import("./bus.ts");
  bus.onChange(() => events.push("emit"));
}

/** Records the call of a method of the runtime, then does what it did. */
function trace<T extends object, K extends keyof T>(target: T, name: K, label: string): void {
  const original = target[name] as unknown as (...args: unknown[]) => unknown;
  (target as Record<K, unknown>)[name] = (...args: unknown[]) => {
    events.push(label);
    return original(...args);
  };
}

async function signedInRuntime(): Promise<Runtime> {
  rig.signedInPhone({ user: "dave" });
  await rig.setOwner("dave");
  return startRuntime();
}

async function startRuntime(): Promise<Runtime> {
  const { getRuntime } = await import("./runtime.ts");
  const rt = await getRuntime();
  // The first pass of the sync controller, which a signed-in start begins, ends before the story.
  await new Promise<void>((resolve) => setTimeout(resolve, 20));
  trace(rt, "revokeSignIn", "revoke");
  trace(rt.sync, "stop", "sync.stop");
  trace(rt.session, "signOut", "session.signOut");
  trace(rt.store, "stopTrip", "store.stopTrip");
  trace(rt.store, "listTrips", "store.listTrips");
  trace(rt.store, "clearAll", "store.clearAll");
  const original = rt.setSignOutUntold.bind(rt);
  rt.setSignOutUntold = (untold) => {
    events.push(`untold:${untold}`);
    original(untold);
  };
  // Whatever the start did is not part of the story.
  events.length = 0;
  return rt;
}

const tripIds = async (rt: Runtime) => (await rt.store.listTrips()).map((t) => t.clientTripId);

beforeEach(newProcess);

describe("Sign out", () => {
  test("starts the request, ends the phone's side, and awaits the answer last", async () => {
    await rig.seedTrip("c1", 3, false); // a trip that is still recording
    const rt = await signedInRuntime();
    rig.world.holdRevoke = true;
    const { signOut } = await import("./actions.ts");

    let finished = false;
    const done = signOut(false).then(() => {
      finished = true;
    });
    // The phone is done with its own side while the instance has not answered yet.
    await vi.waitFor(() => expect(events.filter((e) => e === "emit")).toHaveLength(2));
    expect(finished).toBe(false);
    expect(events).toEqual([
      "revoke",
      "sync.stop",
      "recorder.stop",
      "store.stopTrip",
      "store.listTrips",
      "session.signOut",
      "emit", // the session announces that nobody is signed in
      "store.clearAll",
      "emit", // leaveSession announces the end
    ]);
    expect(rt.session.status()).toBe("signed_out");
    expect(await tripIds(rt)).toEqual([]);

    await vi.waitFor(() => expect(rig.world.releaseRevoke).not.toBeNull());
    rig.world.releaseRevoke!();
    await done;
    expect(events.at(-1)).toBe("untold:false");
    expect(rt.signOutUntold()).toBe(false);
  });

  test("sends the refresh token that the phone held before it forgot it", async () => {
    const rt = await signedInRuntime();
    const { signOut } = await import("./actions.ts");
    await signOut(false);
    const [call] = rig.callsTo("/oauth/revoke");
    expect(Object.fromEntries(new URLSearchParams(call!.body))).toMatchObject({
      token: "ref-1",
      client_id: "client-1",
    });
    expect(rt.session.refreshToken()).toBeNull();
    expect(rig.world.keychain.has(rig.TOKENS_ITEM)).toBe(false);
    // The server address stays, so that the next sign-in can use the same client.
    expect(rig.world.keychain.has(rig.SERVER_ITEM)).toBe(true);
  });

  test("keeps the trips that are not completely on the server when the person says so", async () => {
    await rig.seedTrip("c1", 3, true);
    const rt = await signedInRuntime();
    const binding = rt.binding();
    const { signOut } = await import("./actions.ts");
    await signOut(true);
    expect(events).not.toContain("store.clearAll");
    expect(await tripIds(rt)).toEqual(["c1"]);
    expect(await rt.store.pointCount("c1")).toBe(3);
    // The outbox stays bound to its owner.
    expect(rt.binding()).toBe(binding);
    expect(await rt.store.getMeta("owner")).toBe(
      JSON.stringify({ origin: rig.BASE, user: "dave" }),
    );
  });

  test("deletes them when the person says so", async () => {
    await rig.seedTrip("c1", 3, true);
    const rt = await signedInRuntime();
    const { signOut } = await import("./actions.ts");
    await signOut(false);
    expect(events).toContain("store.clearAll");
    expect(await tripIds(rt)).toEqual([]);
  });

  test("clears the whole local copy when nothing is unsent, also if the person said keep", async () => {
    const rt = await signedInRuntime();
    const { signOut } = await import("./actions.ts");
    await signOut(true);
    expect(events).toContain("store.clearAll");
    expect(await tripIds(rt)).toEqual([]);
  });

  test("deletes nothing when the tokens could not be removed, and says so", async () => {
    await rig.seedTrip("c1", 3, true);
    const rt = await signedInRuntime();
    rig.world.keychainDeleteFails = true;
    const { signOut } = await import("./actions.ts");
    await expect(signOut(false)).rejects.toThrow("The Keystore refused the write");
    expect(events).toContain("session.signOut");
    expect(events).not.toContain("store.clearAll");
    expect(await tripIds(rt)).toEqual(["c1"]);
    expect(rt.binding()).toEqual({ kind: "own" });
  });

  test("says that the instance could not be told when it did not answer", async () => {
    const rt = await signedInRuntime();
    rig.world.offline = true;
    const { signOut } = await import("./actions.ts");
    await signOut(false);
    expect(events.slice(-2)).toEqual(["untold:true", "emit"]);
    expect(rt.signOutUntold()).toBe(true);
  });
});

describe("Remove this phone", () => {
  test("changes nothing on the phone when the instance does not answer with success", async () => {
    await rig.seedTrip("c1", 3, true);
    const rt = await signedInRuntime();
    rig.world.deviceRevokeStatus = 500;
    const { removeThisDevice } = await import("./devices.ts");
    await expect(removeThisDevice(rt, false)).rejects.toThrow();
    expect(rig.callsTo("/api/device/revoke")).toHaveLength(1);
    expect(events).toEqual([]);
    expect(rt.session.status()).toBe("signed_in");
    expect(await tripIds(rt)).toEqual(["c1"]);
  });

  test("signs out as Sign out does once the instance has answered, without a second request to it", async () => {
    await rig.seedTrip("c1", 3, false);
    const rt = await signedInRuntime();
    const { removeThisDevice } = await import("./devices.ts");
    await removeThisDevice(rt, false);
    expect(rig.callsTo("/api/device/revoke")).toHaveLength(1);
    expect(rig.callsTo("/oauth/revoke")).toEqual([]);
    expect(events).toEqual([
      "sync.stop",
      "recorder.stop",
      "store.stopTrip",
      "store.listTrips",
      "session.signOut",
      "emit",
      "store.clearAll",
      "emit",
    ]);
    expect(rt.session.status()).toBe("signed_out");
    expect(await tripIds(rt)).toEqual([]);
  });
});

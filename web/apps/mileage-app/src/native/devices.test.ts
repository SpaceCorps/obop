import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// What the app says to the instance about this phone (`native/devices.ts`): the model `device` and
// its commands `get` and `update`. The model name is persisted in the sense that the instance
// answers to it and to nothing else, so a change of one letter has to fail a test. The command
// `revoke` of the same model is held by `sign-out.test.ts`. The runtime, the session and the
// client are the real ones; the Keychain, the database and the network are stand-ins
// (`test-support/runtime-rig.ts`), and the network here answers these two commands.

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
  stopRecording: async () => undefined,
}));
vi.mock("./permissions.ts", () => ({ locationReadiness: async () => "ready" }));

type Rig = typeof import("../test-support/runtime-rig.ts");
let rig: Rig;
const calls: { path: string; body: string }[] = [];

/** The answers of the instance to `device get` and `device update`; every other path is the rig's. */
async function instance(input: string | Request, init?: RequestInit): Promise<Response> {
  const url = typeof input === "string" ? input : input.url;
  const path = new URL(url).pathname;
  if (path === "/api/device/get" || path === "/api/device/update") {
    let body = "";
    if (typeof input !== "string") body = await input.clone().text();
    else if (typeof init?.body === "string") body = init.body;
    calls.push({ path, body });
    const value =
      path === "/api/device/get"
        ? {
            id: "dev-1",
            name: "Dave's phone",
            platform: "ios",
            app_version: "0.1.0",
            last_seen: "2026-10-15T09:00:00Z",
            state: "active",
          }
        : {};
    return new Response(JSON.stringify({ value }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }
  return rig.network(input, init);
}

async function signedInRuntime() {
  rig.signedInPhone({ user: "dave" });
  await rig.setOwner("dave");
  const { getRuntime } = await import("./runtime.ts");
  const rt = await getRuntime();
  // The first pass of the sync controller, which a signed-in start begins, ends before the story.
  await new Promise<void>((resolve) => setTimeout(resolve, 20));
  return rt;
}

beforeEach(async () => {
  vi.resetModules();
  calls.length = 0;
  rig = await import("../test-support/runtime-rig.ts");
  rig.reset();
  rig.boot();
  vi.stubGlobal("fetch", instance);
  await rig.database();
});

describe("this phone as the instance lists it", () => {
  test("readDevice asks the model device, the command get, for the id of this sign-in", async () => {
    const rt = await signedInRuntime();
    const { readDevice } = await import("./devices.ts");
    const view = await readDevice(rt);
    const asked = calls.filter((call) => call.path === "/api/device/get");
    expect(asked).toHaveLength(1);
    expect(JSON.parse(asked[0]!.body)).toEqual({ id: "dev-1" });
    expect(view).toEqual({
      id: "dev-1",
      name: "Dave's phone",
      platform: "ios",
      appVersion: "0.1.0",
      lastSeen: "2026-10-15T09:00:00Z",
      state: "active",
    });
  });

  test("renameDevice asks the model device, the command update, with the id and the trimmed name", async () => {
    const rt = await signedInRuntime();
    const { renameDevice } = await import("./devices.ts");
    await renameDevice(rt, "  Work phone ");
    const asked = calls.filter((call) => call.path === "/api/device/update");
    expect(asked).toHaveLength(1);
    expect(JSON.parse(asked[0]!.body)).toEqual({ id: "dev-1", name: "Work phone" });
  });
});

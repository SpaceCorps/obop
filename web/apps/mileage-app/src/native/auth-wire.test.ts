import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { TokenSet } from "@obop/client";
import type { Binding, Owner } from "../core/identity.ts";
import { createSession, type ServerConfig, type SessionStorage } from "../core/session.ts";

// What the app says to the instance when it signs in. The instance binds the client it registered
// to the redirect and the name it was given, so an installed app must keep saying exactly these
// words (a stored client id is reused for up to 30 days). The whole path is run: the register
// call, the authorize address, the system browser, the exchange of the code, the question who the
// device belongs to. Only the network, the browser and the platform are stand-ins.

const net = vi.hoisted(() => {
  const state = {
    calls: [] as { method: string; url: string; body: string }[],
    /** The status of the authorization page for the probe. */
    authorizeStatus: 200,
    nextClient: 1,
    ownerUser: "dave",
  };
  const json = (body: unknown, status = 200) =>
    new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
  const handle = async (input: string | Request, init?: RequestInit): Promise<Response> => {
    const url = typeof input === "string" ? input : input.url;
    const method = (typeof input === "string" ? init?.method : input.method) ?? "GET";
    let body = "";
    if (typeof input !== "string") body = await input.clone().text();
    else if (typeof init?.body === "string") body = init.body;
    state.calls.push({ method, url, body });
    const path = new URL(url).pathname;
    if (path === "/oauth/register") return json({ client_id: `client-${state.nextClient++}` }, 201);
    if (path === "/oauth/authorize")
      return new Response("<html></html>", { status: state.authorizeStatus });
    if (path === "/oauth/token") {
      return json({
        access_token: "acc-1",
        refresh_token: "ref-1",
        expires_in: 3600,
        device_id: "dev-1",
        scope: "mileage.driver",
      });
    }
    if (path === "/oauth/revoke") return json({});
    if (path === "/api/device/get") return json({ value: { user: state.ownerUser } });
    return json({ error: "not_found" }, 404);
  };
  return { state, handle };
});

const browser = vi.hoisted(() => ({
  opened: [] as { url: string; redirect: string }[],
  /** What the person does in the browser: `success` follows the redirect with a code. */
  outcome: "success" as "success" | "cancel",
}));

vi.mock("expo-web-browser", () => ({
  openAuthSessionAsync: async (url: string, redirect: string) => {
    browser.opened.push({ url, redirect });
    if (browser.outcome === "cancel") return { type: "cancel" };
    const state = new URL(url).searchParams.get("state");
    return { type: "success", url: `${redirect}?code=code-1&state=${state}` };
  },
}));
vi.mock("expo-constants", () => ({ default: { expoConfig: { extra: {} } } }));
vi.mock("./platform-deps.ts", async () => {
  const { createHash } = await import("node:crypto");
  return {
    oauthDeps: {
      sha256: async (text: string) => new Uint8Array(createHash("sha256").update(text).digest()),
      randomBytes: async (count: number) => new Uint8Array(count).map((_, i) => (i * 7 + 3) % 256),
      fetch: net.handle,
    },
    deviceInfo: () => ({ name: "Test phone", platform: "ios", appVersion: "0.1.0" }),
    appVersion: () => "0.1.0",
  };
});

const { signIn, SignInError } = await import("./auth.ts");
const { oauthDeps } = await import("./platform-deps.ts");

const BASE = "https://phone.example";

function memoryStorage(config: ServerConfig | null = null) {
  const saved = { config, tokens: null as TokenSet | null };
  const storage: SessionStorage = {
    loadConfig: async () => saved.config,
    saveConfig: async (c) => {
      saved.config = c;
    },
    loadTokens: async () => saved.tokens,
    saveTokens: async (t) => {
      saved.tokens = t;
    },
  };
  return { saved, storage };
}

async function rig(config: ServerConfig | null = null) {
  const { saved, storage } = memoryStorage(config);
  const session = createSession({ storage, oauth: oauthDeps });
  await session.restore();
  const bound: Owner[] = [];
  const host = {
    session,
    bindIdentity: async (owner: Owner): Promise<Binding> => {
      bound.push(owner);
      return { kind: "own" };
    },
  };
  return { saved, session, host, bound };
}

const callsTo = (path: string) => net.state.calls.filter((c) => new URL(c.url).pathname === path);

beforeEach(() => {
  net.state.calls.length = 0;
  net.state.authorizeStatus = 200;
  net.state.nextClient = 1;
  net.state.ownerUser = "dave";
  browser.opened.length = 0;
  browser.outcome = "success";
  vi.stubGlobal("fetch", net.handle);
});

describe("the registration and the address of the sign-in page", () => {
  test("a new client is registered with the name, the redirect and the groups that earlier builds sent", async () => {
    browser.outcome = "cancel";
    const { host } = await rig();
    await expect(signIn(host, BASE)).rejects.toMatchObject({ reason: "cancelled" });

    const [register] = callsTo("/oauth/register");
    expect(register?.method).toBe("POST");
    expect(JSON.parse(register!.body)).toEqual({
      client_name: "obop Mileage",
      redirect_uris: ["org.obop.mileage:/oauth"],
      grant_types: ["authorization_code", "refresh_token"],
      response_types: ["code"],
      token_endpoint_auth_method: "none",
      obop_client_class: "mobile",
      scope: "mileage.driver",
    });

    expect(browser.opened).toHaveLength(1);
    const { url, redirect } = browser.opened[0]!;
    expect(redirect).toBe("org.obop.mileage:/oauth");
    const address = new URL(url);
    expect(address.origin + address.pathname).toBe(`${BASE}/oauth/authorize`);
    expect(Object.fromEntries(address.searchParams)).toMatchObject({
      response_type: "code",
      client_id: "client-1",
      redirect_uri: "org.obop.mileage:/oauth",
      code_challenge_method: "S256",
      scope: "mileage.driver",
    });
    expect(address.searchParams.get("state")).toMatch(/\S/);
  });

  test("the address is not accepted as it is typed when it is not an address", async () => {
    const { host } = await rig();
    await expect(signIn(host, "not an address")).rejects.toBeInstanceOf(SignInError);
    expect(net.state.calls).toEqual([]);
    expect(browser.opened).toEqual([]);
  });
});

describe("a sign-in that is completed", () => {
  test("the exchange carries the same redirect, the client and the verifier of the challenge", async () => {
    const { host, bound, saved, session } = await rig();
    await signIn(host, BASE);

    const [token] = callsTo("/oauth/token");
    const form = new URLSearchParams(token!.body);
    expect(Object.fromEntries(form)).toMatchObject({
      grant_type: "authorization_code",
      code: "code-1",
      client_id: "client-1",
      redirect_uri: "org.obop.mileage:/oauth",
      device_name: "Test phone",
      device_platform: "ios",
      device_app_version: "0.1.0",
    });
    // The verifier is the one of the challenge in the address (PKCE S256).
    const challenge = new URL(browser.opened[0]!.url).searchParams.get("code_challenge");
    const { createHash } = await import("node:crypto");
    const verifier = form.get("code_verifier")!;
    expect(createHash("sha256").update(verifier).digest("base64url")).toBe(challenge);

    // The person was asked who the device belongs to, with the token of this sign-in.
    expect(callsTo("/api/device/get")).toHaveLength(1);
    expect(bound).toEqual([{ origin: BASE, user: "dave" }]);
    expect(session.status()).toBe("signed_in");
    expect(saved.tokens?.refreshToken).toBe("ref-1");
    expect(saved.config).toMatchObject({ baseUrl: BASE, clientId: "client-1", user: "dave" });
    expect(typeof saved.config?.usedAt).toBe("number");
  });

  test("a client of an earlier sign-in is used again without a new registration", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "old-client",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    const { host, saved } = await rig(stored);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toEqual([]);
    expect(new URL(browser.opened[0]!.url).searchParams.get("client_id")).toBe("old-client");
    expect(new URLSearchParams(callsTo("/oauth/token")[0]!.body).get("client_id")).toBe(
      "old-client",
    );
    expect(callsTo("/oauth/authorize")).toHaveLength(1); // the probe
    expect(saved.config?.clientId).toBe("old-client");
  });

  test("an instance that has forgotten the client gets a new registration once", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "forgotten",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    net.state.authorizeStatus = 400;
    const { host, saved } = await rig(stored);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toHaveLength(1);
    expect(new URL(browser.opened[0]!.url).searchParams.get("client_id")).toBe("client-1");
    expect(saved.config?.clientId).toBe("client-1");
  });
});

describe("a sign-in that cannot be finished", () => {
  test("is ended at the instance with the refresh token and the client, and reported as failed", async () => {
    const { host, session } = await rig();
    host.bindIdentity = async () => {
      throw new Error("the outbox is broken");
    };
    await expect(signIn(host, BASE)).rejects.toMatchObject({ reason: "failed" });
    const [revoke] = callsTo("/oauth/revoke");
    expect(Object.fromEntries(new URLSearchParams(revoke!.body))).toMatchObject({
      token: "ref-1",
      client_id: "client-1",
    });
    expect(session.status()).toBe("signed_out");
  });
});

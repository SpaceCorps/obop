import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { TokenSet } from "@obop/client";
import type { Binding, Owner } from "../core/identity.ts";
import { createSession, type ServerConfig, type SessionStorage } from "../core/session.ts";
import { defineStartup, type StartupConfig } from "../core/startup-config.ts";

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
    /** The error of the JSON answer to the probe (a 400 of an instance that knows JSON). */
    authorizeError: "invalid_client",
    /** Whether the instance answers the probe in JSON when asked (since 2026-11-17), or with its page. */
    authorizeJson: true,
    /** The Accept header of each read of the authorization page. */
    authorizeAccept: [] as (string | null)[],
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
    if (path === "/oauth/authorize") {
      const accept = new Headers(typeof input === "string" ? init?.headers : input.headers).get(
        "accept",
      );
      state.authorizeAccept.push(accept);
      if (state.authorizeJson && accept?.includes("application/json")) {
        return state.authorizeStatus === 200
          ? json({ status: "ok" })
          : json({ error: state.authorizeError }, state.authorizeStatus);
      }
      return new Response("<html></html>", { status: state.authorizeStatus });
    }
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
  outcome: "success" as "success" | "cancel" | "killed",
  /** The address the app was started with (a redirect after the system ended the app), or null. */
  initialUrl: null as string | null,
  /** What the secure store holds (a stand-in for the Keystore and the Keychain). */
  items: new Map<string, string>(),
}));

vi.mock("expo-web-browser", () => ({
  openAuthSessionAsync: async (url: string, redirect: string) => {
    browser.opened.push({ url, redirect });
    if (browser.outcome === "cancel") return { type: "cancel" };
    const state = new URL(url).searchParams.get("state");
    if (browser.outcome === "killed") {
      // The system ends the app while the browser is in front; the redirect starts it again.
      // Nothing of the old process runs after this: its promise never settles.
      browser.initialUrl = `${redirect}?code=code-1&state=${state}`;
      return new Promise<never>(() => undefined);
    }
    return { type: "success", url: `${redirect}?code=code-1&state=${state}` };
  },
}));
vi.mock("expo-constants", () => ({ default: { expoConfig: { extra: {} } } }));

// The language store (`native/language.ts`) is the real one. The two things it needs from a phone
// are stand-ins: the languages the phone reports, and the secure store that keeps the choice.
const phone = vi.hoisted(() => ({ languages: [] as string[] }));
vi.mock("./phone-language.ts", () => ({ phoneLanguageTags: () => phone.languages }));
vi.mock("./keychain.ts", () => ({
  keychain: {
    get: async (name: string) => browser.items.get(name) ?? null,
    set: async (name: string, value: string) => {
      browser.items.set(name, value);
    },
    delete: async (name: string) => {
      browser.items.delete(name);
    },
  },
}));

vi.mock("./platform-deps.ts", async () => {
  const { createHash } = await import("node:crypto");
  return {
    oauthDeps: {
      sha256: async (text: string) => new Uint8Array(createHash("sha256").update(text).digest()),
      randomBytes: async (count: number) => new Uint8Array(count).map((_, i) => (i * 7 + 3) % 256),
      fetch: net.handle,
    },
    deviceInfo: () => ({ name: "Test phone", platform: "ios", appVersion: "0.1.0" }),
    initialUrl: async () => browser.initialUrl,
    appVersion: () => "0.1.0",
  };
});

const { resumeSignIn, signIn, SignInError } = await import("./auth.ts");
const { startup } = await import("../../config/startup.ts");
const { identity } = await import("../../config/identity.ts");
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

async function rig(config: ServerConfig | null = null, configuration: StartupConfig = startup) {
  const { saved, storage } = memoryStorage(config);
  const session = createSession({ storage, oauth: oauthDeps });
  await session.restore();
  const bound: Owner[] = [];
  const host = {
    startup: configuration,
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
  net.state.authorizeError = "invalid_client";
  net.state.authorizeJson = true;
  net.state.authorizeAccept.length = 0;
  net.state.nextClient = 1;
  net.state.ownerUser = "dave";
  browser.opened.length = 0;
  browser.outcome = "success";
  browser.initialUrl = null;
  browser.items.clear();
  phone.languages = [];
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

  test("the pages are asked for the language the app knows, and for none when it knows none", async () => {
    browser.outcome = "cancel";
    const lang = async (languages: string[]) => {
      phone.languages = languages;
      browser.opened.length = 0;
      const { host } = await rig();
      await expect(signIn(host, BASE)).rejects.toMatchObject({ reason: "cancelled" });
      return new URL(browser.opened[0]!.url).searchParams.get("lang");
    };
    // A phone in a language the app has: the pages are asked for it.
    expect(await lang(["sv-SE", "en-SE"])).toBe("sv");
    // A phone in a language the app does not have, or one that reports none: no `lang`, the
    // browser's own language decides (docs/spec/mobile.md 8.9).
    expect(await lang(["de-DE", "sv-SE"])).toBeNull();
    expect(await lang([])).toBeNull();
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
    // The page was asked for JSON: no sign-in flow is kept by the instance for the question.
    expect(net.state.authorizeAccept[0]).toBe("application/json");
  });

  test("a refusal of another kind is not taken for a forgotten client", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "known",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    net.state.authorizeStatus = 400;
    net.state.authorizeError = "invalid_request";
    const { host } = await rig(stored);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toEqual([]);
    expect(new URL(browser.opened[0]!.url).searchParams.get("client_id")).toBe("known");
  });

  test("an older instance that answers with its error page still gets a new registration", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "forgotten",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    net.state.authorizeStatus = 400;
    net.state.authorizeJson = false;
    const { host, saved } = await rig(stored);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toHaveLength(1);
    expect(saved.config?.clientId).toBe("client-1");
  });
});

describe("the groups the client is registered with", () => {
  const MILEAGE = "mileage.driver";

  test("are stored with the address when the client is registered, and kept by the sign-in", async () => {
    const { host, saved } = await rig();
    await signIn(host, BASE);
    expect(saved.config).toMatchObject({ clientId: "client-1", groups: [MILEAGE], user: "dave" });
  });

  test("a client of an earlier build, stored with no list, is not registered again", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "old-client",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    const { host, saved } = await rig(stored);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toEqual([]);
    // Nothing is added to what the earlier build stored but the use.
    expect(saved.config).toEqual({ ...stored, usedAt: saved.config?.usedAt });
  });

  test("a build that asks for one more group registers a new client at the next sign-in", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "old-client",
      user: "dave",
      usedAt: Date.now() - 1000,
    };
    const grown = defineStartup({
      identity,
      groups: [MILEAGE, "fleet.driver"],
      legacyGroups: startup.legacyGroups,
    });
    const { host, saved } = await rig(stored, grown);
    await signIn(host, BASE);
    const [register] = callsTo("/oauth/register");
    expect(JSON.parse(register!.body).scope).toBe("mileage.driver fleet.driver");
    const address = new URL(browser.opened[0]!.url);
    expect(address.searchParams.get("client_id")).toBe("client-1");
    expect(address.searchParams.get("scope")).toBe("mileage.driver fleet.driver");
    expect(saved.config).toMatchObject({
      clientId: "client-1",
      groups: [MILEAGE, "fleet.driver"],
    });
  });

  test("a client registered with the list the app asks for is used again", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "known-client",
      user: "dave",
      usedAt: Date.now() - 1000,
      groups: [MILEAGE, "fleet.driver"],
    };
    const grown = defineStartup({ identity, groups: ["fleet.driver", MILEAGE] });
    const { host } = await rig(stored, grown);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toEqual([]);
    expect(new URL(browser.opened[0]!.url).searchParams.get("client_id")).toBe("known-client");
  });

  test("an app with no history of its own registers when it finds a config it cannot account for", async () => {
    const stored: ServerConfig = {
      baseUrl: BASE,
      clientId: "old-client",
      usedAt: Date.now() - 1000,
    };
    const fresh = defineStartup({ identity, groups: [MILEAGE] });
    const { host } = await rig(stored, fresh);
    await signIn(host, BASE);
    expect(callsTo("/oauth/register")).toHaveLength(1);
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

describe("a sign-in that the system ended the app in the middle of (L32)", () => {
  const PENDING = "obop.mileage.server.pending-sign-in";

  /** Begins a sign-in that the system ends while the browser is in front, and starts the app again. */
  async function killedSignIn() {
    browser.outcome = "killed";
    const first = await rig();
    void signIn(first.host, BASE);
    await vi.waitFor(() => expect(browser.opened).toHaveLength(1));
    // The process is gone: a new session reads what the old one stored.
    const again = await rig(first.saved.config);
    return { first, again };
  }

  test("is kept in the secure store while the browser is open, and removed when the sign-in ends", async () => {
    // Written before the browser opens, with the state of the address it opens.
    await killedSignIn();
    const stored = JSON.parse(browser.items.get(PENDING)!);
    expect(stored.state).toBe(new URL(browser.opened[0]!.url).searchParams.get("state"));
    expect(stored.config).toMatchObject({ baseUrl: BASE, clientId: "client-1" });
    // A cancelled sign-in removes it.
    browser.items.clear();
    browser.outcome = "cancel";
    const cancelled = await rig();
    await expect(signIn(cancelled.host, BASE)).rejects.toMatchObject({ reason: "cancelled" });
    expect(browser.items.has(PENDING)).toBe(false);
    // A completed sign-in removes it too.
    browser.outcome = "success";
    const done = await rig();
    await signIn(done.host, BASE);
    expect(browser.items.has(PENDING)).toBe(false);
  });

  test("is finished from the redirect that started the app again, with the same client and verifier", async () => {
    const { again } = await killedSignIn();
    expect(browser.items.has(PENDING)).toBe(true);
    let resumed = false;
    expect(await resumeSignIn(again.host, () => (resumed = true))).toBe(true);
    expect(resumed).toBe(true);
    expect(again.session.status()).toBe("signed_in");
    expect(again.bound).toEqual([{ origin: BASE, user: "dave" }]);
    const form = new URLSearchParams(callsTo("/oauth/token")[0]!.body);
    expect(form.get("client_id")).toBe("client-1");
    expect(form.get("code")).toBe("code-1");
    const challenge = new URL(browser.opened[0]!.url).searchParams.get("code_challenge");
    const { createHash } = await import("node:crypto");
    expect(createHash("sha256").update(form.get("code_verifier")!).digest("base64url")).toBe(
      challenge,
    );
    expect(browser.items.has(PENDING)).toBe(false);
    // Tried once: a second start finds nothing to finish.
    expect(await resumeSignIn(again.host)).toBe(false);
    expect(callsTo("/oauth/token")).toHaveLength(1);
  });

  test("is dropped, and not finished, when the app was started another way or with another state", async () => {
    for (const started of [null, "org.obop.mileage:/oauth?code=code-1&state=forged"]) {
      net.state.calls.length = 0;
      const { again } = await killedSignIn();
      browser.initialUrl = started;
      expect(await resumeSignIn(again.host)).toBe(false);
      expect(callsTo("/oauth/token")).toEqual([]);
      expect(again.session.status()).toBe("signed_out");
      expect(browser.items.has(PENDING)).toBe(false);
    }
  });

  test("a refused exchange is reported as a failed sign-in and the pending one is removed", async () => {
    const { again } = await killedSignIn();
    browser.initialUrl =
      "org.obop.mileage:/oauth?error=access_denied&state=" +
      new URL(browser.opened[0]!.url).searchParams.get("state");
    await expect(resumeSignIn(again.host)).rejects.toMatchObject({ reason: "denied" });
    expect(browser.items.has(PENDING)).toBe(false);
  });
});

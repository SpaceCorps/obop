import { createHash, randomBytes as nodeRandom } from "node:crypto";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import {
  OAuthError,
  base64Url,
  buildAuthorizeUrl,
  createPkce,
  createState,
  exchangeCode,
  normaliseBaseUrl,
  parseAuthorizationResponse,
  refreshTokens,
  registerMobileClient,
  revokeToken,
  type OAuthDeps,
} from "./index.ts";

const sha256 = async (text: string) => new Uint8Array(createHash("sha256").update(text).digest());
const randomBytes = async (n: number) => new Uint8Array(nodeRandom(n));

function deps(fetchImpl?: (url: string, init: RequestInit) => Response | Promise<Response>) {
  const calls: { url: string; init: RequestInit }[] = [];
  const fetch = vi.fn(async (input: string, init?: RequestInit) => {
    const url = input;
    calls.push({ url, init: init ?? {} });
    if (!fetchImpl) throw new Error("no fetch expected");
    return fetchImpl(url, init ?? {});
  });
  const value: OAuthDeps = {
    sha256,
    randomBytes,
    fetch: fetch as unknown as typeof globalThis.fetch,
  };
  return { deps: value, calls };
}

/** The text of a request body; the helper sends strings only. */
const bodyOf = (init: RequestInit): string => {
  if (typeof init.body !== "string") throw new Error("the body is not a string");
  return init.body;
};

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("base64Url and PKCE", () => {
  test("base64Url matches the RFC 4648 vectors, without padding", () => {
    const enc = (s: string) => base64Url(new TextEncoder().encode(s));
    expect(enc("")).toBe("");
    expect(enc("f")).toBe("Zg");
    expect(enc("fo")).toBe("Zm8");
    expect(enc("foo")).toBe("Zm9v");
    expect(enc("foob")).toBe("Zm9vYg");
    expect(base64Url(new Uint8Array([0xfb, 0xff, 0xfe]))).toBe("-__-");
  });

  test("the challenge of the RFC 7636 appendix B verifier is the published one", async () => {
    const published = {
      verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
      challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
    };
    expect(base64Url(await sha256(published.verifier))).toBe(published.challenge);
  });

  test("a pair has a 43 character verifier and the S256 challenge of it", async () => {
    const pair = await createPkce({ sha256, randomBytes });
    expect(pair.verifier).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(pair.challenge).toBe(base64Url(await sha256(pair.verifier)));
    expect(pair.method).toBe("S256");
    expect((await createPkce({ sha256, randomBytes })).verifier).not.toBe(pair.verifier);
  });

  test("a state is random and URL safe", async () => {
    const a = await createState({ randomBytes });
    expect(a).toMatch(/^[A-Za-z0-9_-]{22}$/);
    expect(await createState({ randomBytes })).not.toBe(a);
  });
});

describe("the authorization address", () => {
  test("carries PKCE S256, the redirect, the state and the scope, and nothing secret", () => {
    const url = new URL(
      buildAuthorizeUrl({
        baseUrl: "http://10.0.2.2:8080/",
        clientId: "client-1",
        redirectUri: "org.obop.mileage:/oauth",
        challenge: "CH",
        state: "ST",
        scope: ["mileage.driver", "base.user"],
      }),
    );
    expect(url.origin + url.pathname).toBe("http://10.0.2.2:8080/oauth/authorize");
    expect(Object.fromEntries(url.searchParams)).toEqual({
      response_type: "code",
      client_id: "client-1",
      redirect_uri: "org.obop.mileage:/oauth",
      code_challenge: "CH",
      code_challenge_method: "S256",
      state: "ST",
      scope: "mileage.driver base.user",
    });
  });

  test("normaliseBaseUrl drops spaces and trailing slashes", () => {
    expect(normaliseBaseUrl("  https://acme.example.com// ")).toBe("https://acme.example.com");
  });
});

describe("the redirect", () => {
  const expected = { state: "ST", baseUrl: "https://acme.example.com" };

  test("gives the code", () => {
    expect(
      parseAuthorizationResponse("org.obop.mileage:/oauth?code=abc%2B1&state=ST", expected),
    ).toBe("abc+1");
  });

  test("refuses another state, another issuer, an error and a missing code", () => {
    const code = (url: string) => {
      try {
        parseAuthorizationResponse(url, expected);
      } catch (e) {
        return e instanceof OAuthError ? e.code : "other";
      }
      return "none";
    };
    expect(code("org.obop.mileage:/oauth?code=abc&state=OTHER")).toBe("state_mismatch");
    expect(code("org.obop.mileage:/oauth?code=abc")).toBe("state_mismatch");
    expect(code("org.obop.mileage:/oauth?code=abc&state=ST&iss=https%3A%2F%2Fevil.example")).toBe(
      "issuer_mismatch",
    );
    expect(code("org.obop.mileage:/oauth?error=access_denied&state=ST")).toBe("access_denied");
    // A link that any other app opens, with an error and without this sign-in's state, is read as
    // a foreign answer, never as the person's refusal. (It still fails the attempt that waits: the
    // browser wrapper of the platform hands it to the waiting sign-in. What it cannot do is give a
    // code that could be exchanged.)
    expect(code("org.obop.mileage:/oauth?error=access_denied")).toBe("state_mismatch");
    expect(code("org.obop.mileage:/oauth?error=access_denied&state=OTHER")).toBe("state_mismatch");
    expect(code("org.obop.mileage:/oauth?state=ST")).toBe("invalid_response");
    expect(
      code("org.obop.mileage:/oauth?code=abc&state=ST&iss=https%3A%2F%2Facme.example.com%2F"),
    ).toBe("none");
  });

  test("the first of a repeated parameter wins", () => {
    expect(
      parseAuthorizationResponse(
        "org.obop.mileage:/oauth?code=first&code=second&state=ST",
        expected,
      ),
    ).toBe("first");
  });
});

describe("registration", () => {
  test("posts a public client of the mobile class and answers the client id", async () => {
    const { deps: d, calls } = deps(() => json({ client_id: "cid-1" }, 201));
    const id = await registerMobileClient(d, {
      baseUrl: "http://10.0.2.2:8080",
      redirectUri: "org.obop.mileage:/oauth",
      name: "Obop Mileage",
      scope: ["mileage.driver"],
    });
    expect(id).toBe("cid-1");
    expect(calls[0]!.url).toBe("http://10.0.2.2:8080/oauth/register");
    expect(JSON.parse(bodyOf(calls[0]!.init))).toMatchObject({
      client_name: "Obop Mileage",
      redirect_uris: ["org.obop.mileage:/oauth"],
      token_endpoint_auth_method: "none",
      obop_client_class: "mobile",
      scope: "mileage.driver",
    });
  });

  test("a refusal keeps the server's error word", async () => {
    const { deps: d } = deps(() =>
      json({ error: "invalid_redirect_uri", error_description: "no https" }, 400),
    );
    await expect(
      registerMobileClient(d, {
        baseUrl: "https://x.example",
        redirectUri: "https://x.example/cb",
        name: "n",
        scope: [],
      }),
    ).rejects.toMatchObject({ code: "invalid_redirect_uri", status: 400 });
  });
});

describe("tokens", () => {
  const answer = {
    access_token: "AT-1",
    refresh_token: "RT-1",
    expires_in: 3600,
    scope: "mileage.driver base.user",
    device_id: "dev-9",
    token_type: "Bearer",
  };

  test("the exchange sends the code, the verifier and the device, and reads the answer", async () => {
    const { deps: d, calls } = deps(() => json(answer));
    const set = await exchangeCode(d, {
      baseUrl: "http://10.0.2.2:8080",
      clientId: "cid-1",
      redirectUri: "org.obop.mileage:/oauth",
      code: "CODE",
      verifier: "VER",
      device: { name: "Pixel 9", platform: "android", appVersion: "0.1.0" },
      now: 1_000,
    });
    expect(set).toEqual({
      accessToken: "AT-1",
      refreshToken: "RT-1",
      expiresAt: 1_000 + 3_600_000,
      deviceId: "dev-9",
      scope: ["mileage.driver", "base.user"],
    });
    const body = new URLSearchParams(bodyOf(calls[0]!.init));
    expect(Object.fromEntries(body)).toEqual({
      grant_type: "authorization_code",
      code: "CODE",
      client_id: "cid-1",
      redirect_uri: "org.obop.mileage:/oauth",
      code_verifier: "VER",
      device_name: "Pixel 9",
      device_platform: "android",
      device_app_version: "0.1.0",
    });
    expect((calls[0]!.init.headers as Record<string, string>)["content-type"]).toBe(
      "application/x-www-form-urlencoded",
    );
  });

  test("a refresh sends the refresh token and answers the next pair", async () => {
    const { deps: d, calls } = deps(() => json({ ...answer, refresh_token: "RT-2" }));
    const set = await refreshTokens(d, {
      baseUrl: "http://h",
      clientId: "cid-1",
      refreshToken: "RT-1",
      now: 0,
    });
    expect(set.refreshToken).toBe("RT-2");
    expect(Object.fromEntries(new URLSearchParams(bodyOf(calls[0]!.init)))).toEqual({
      grant_type: "refresh_token",
      refresh_token: "RT-1",
      client_id: "cid-1",
    });
  });

  test("an invalid grant is a typed error and never repeats the token", async () => {
    const { deps: d } = deps(() => json({ error: "invalid_grant" }, 400));
    const error = await refreshTokens(d, {
      baseUrl: "http://h",
      clientId: "c",
      refreshToken: "SECRET-RT",
    }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(OAuthError);
    expect((error as OAuthError).code).toBe("invalid_grant");
    expect(String(error)).not.toContain("SECRET-RT");
  });

  test("no connection is network_error with status 0, and a page that is not JSON is invalid_response", async () => {
    const down = deps(() => {
      throw new TypeError("Network request failed");
    });
    await expect(
      refreshTokens(down.deps, { baseUrl: "http://h", clientId: "c", refreshToken: "r" }),
    ).rejects.toMatchObject({ code: "network_error", status: 0 });
    const html = deps(() => new Response("<html></html>", { status: 200 }));
    await expect(
      refreshTokens(html.deps, { baseUrl: "http://h", clientId: "c", refreshToken: "r" }),
    ).rejects.toMatchObject({ code: "invalid_response" });
    const partial = deps(() => json({ access_token: "a" }));
    await expect(
      refreshTokens(partial.deps, { baseUrl: "http://h", clientId: "c", refreshToken: "r" }),
    ).rejects.toMatchObject({ code: "invalid_response" });
  });
});

describe("revocation", () => {
  test("posts the refresh token and the client id, with the hint", async () => {
    const { deps: d, calls } = deps(() => json({}));
    await revokeToken(d, { baseUrl: "http://h/", clientId: "cid-1", token: "RT-1" });
    expect(calls[0]!.url).toBe("http://h/oauth/revoke");
    expect(Object.fromEntries(new URLSearchParams(bodyOf(calls[0]!.init)))).toEqual({
      token: "RT-1",
      token_type_hint: "refresh_token",
      client_id: "cid-1",
    });
  });

  test("a call that is not answered in time is given up as network_error", async () => {
    const never = deps(
      (_url, init) =>
        new Promise<Response>((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(new Error("aborted")));
        }),
    );
    const started = Date.now();
    await expect(
      revokeToken(never.deps, { baseUrl: "http://h", clientId: "c", token: "t", timeoutMs: 40 }),
    ).rejects.toMatchObject({ code: "network_error" });
    expect(Date.now() - started).toBeLessThan(2000);
  });

  test("a call that timed out says so, and one that failed to connect does not", async () => {
    const never = deps(() => new Promise<Response>(() => {}));
    const slow = await revokeToken(never.deps, {
      baseUrl: "http://h",
      clientId: "c",
      token: "t",
      timeoutMs: 20,
    }).catch((e: unknown) => e);
    expect(slow).toMatchObject({ code: "network_error", timedOut: true });
    const down = deps(() => {
      throw new TypeError("Network request failed");
    });
    const gone = await revokeToken(down.deps, {
      baseUrl: "http://h",
      clientId: "c",
      token: "t",
    }).catch((e: unknown) => e);
    expect(gone).toMatchObject({ code: "network_error", timedOut: false });
  });

  test("an answer that starts and then stalls is given up too", async () => {
    const stalledBody = deps(
      async () =>
        ({
          ok: true,
          status: 200,
          statusText: "OK",
          text: () => new Promise<string>(() => {}),
        }) as unknown as Response,
    );
    const started = Date.now();
    await expect(
      revokeToken(stalledBody.deps, {
        baseUrl: "http://h",
        clientId: "c",
        token: "t",
        timeoutMs: 40,
      }),
    ).rejects.toMatchObject({ code: "network_error", timedOut: true });
    expect(Date.now() - started).toBeLessThan(2000);
  });

  test("a refresh takes its own time limit", async () => {
    const never = deps(() => new Promise<Response>(() => {}));
    await expect(
      refreshTokens(never.deps, {
        baseUrl: "http://h",
        clientId: "c",
        refreshToken: "RT",
        timeoutMs: 20,
      }),
    ).rejects.toMatchObject({ timedOut: true });
  });

  test("a fetch that ignores the abort signal is given up as well", async () => {
    const never = deps(() => new Promise<Response>(() => {}));
    await expect(
      revokeToken(never.deps, { baseUrl: "http://h", clientId: "c", token: "t", timeoutMs: 40 }),
    ).rejects.toMatchObject({ code: "network_error" });
  });

  test("no connection is network_error", async () => {
    const down = deps(() => {
      throw new TypeError("Network request failed");
    });
    await expect(
      revokeToken(down.deps, { baseUrl: "http://h", clientId: "c", token: "t" }),
    ).rejects.toMatchObject({ code: "network_error" });
  });
});

describe("a deadline that fires after the app was suspended", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  const answer = {
    access_token: "AT-2",
    refresh_token: "RT-2",
    expires_in: 3600,
    scope: "mileage.driver",
    device_id: "dev-9",
  };

  /** A fetch whose answer the test lets through when it wants to. */
  function held() {
    let release: (response: Response) => void = () => undefined;
    const d = deps(
      () =>
        new Promise<Response>((resolve) => {
          release = resolve;
        }),
    );
    return { ...d, release: (response: Response) => release(response) };
  }

  const refresh = (d: OAuthDeps) =>
    refreshTokens(d, {
      baseUrl: "http://h",
      clientId: "c",
      refreshToken: "RT-1",
      timeoutMs: 60_000,
    });

  test("an app that slept through the deadline gives the answer that is on its way a short time more", async () => {
    vi.useFakeTimers();
    const call = held();
    const result = refresh(call.deps);
    const settled = vi.fn();
    result.then(settled, settled);
    // The app is suspended for ten minutes: the clock moves on, no timer runs, and on waking the
    // deadline of 60 seconds fires at once.
    vi.setSystemTime(Date.now() + 600_000);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(settled).not.toHaveBeenCalled();
    // The answer arrives in the grace period.
    call.release(json(answer));
    const set = await result;
    expect(set.refreshToken).toBe("RT-2");
  });

  test("the grace is short and comes once: with no answer in it the call is given up as timed out", async () => {
    vi.useFakeTimers();
    const call = held();
    const result = refresh(call.deps);
    const failure = result.catch((e: unknown) => e);
    vi.setSystemTime(Date.now() + 600_000);
    await vi.advanceTimersByTimeAsync(60_000);
    await vi.advanceTimersByTimeAsync(9_999);
    let early = true;
    void failure.then(() => (early = false));
    await Promise.resolve();
    expect(early).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    expect(await failure).toMatchObject({ code: "network_error", timedOut: true });
  });

  test("a deadline that fires on time is not given more time", async () => {
    vi.useFakeTimers();
    const call = held();
    const failure = refresh(call.deps).catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(59_999);
    let early = true;
    void failure.then(() => (early = false));
    await Promise.resolve();
    expect(early).toBe(true);
    await vi.advanceTimersByTimeAsync(1);
    expect(await failure).toMatchObject({ code: "network_error", timedOut: true });
  });

  test("a small delay of the timer is not a nap", async () => {
    vi.useFakeTimers();
    const call = held();
    const failure = refresh(call.deps).catch((e: unknown) => e);
    // A busy moment: the clock is 4 seconds ahead of the timers when the deadline runs.
    vi.setSystemTime(Date.now() + 4_000);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(await failure).toMatchObject({ code: "network_error", timedOut: true });
  });
});

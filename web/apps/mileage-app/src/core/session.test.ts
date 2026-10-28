import { OAuthError, type OAuthDeps } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import {
  CLIENT_FRESH_MS,
  createSession,
  clientUnknown,
  needsRegistration,
  type SessionStorage,
} from "./session.ts";

const tokens = (n: number, expiresAt: number) => ({
  accessToken: `AT-${n}`,
  refreshToken: `RT-${n}`,
  expiresAt,
  deviceId: "dev-1",
  scope: ["mileage.driver"],
});

function rig(
  initial = tokens(1, 100_000),
  options: { refreshTimeoutMs?: number; onEndUntold?: () => void } = {},
) {
  const saved: (ReturnType<typeof tokens> | null)[] = [];
  const configs: unknown[] = [];
  const urls: string[] = [];
  let clock = 0;
  let saveFails = 0;
  let holdSave: Promise<void> | null = null;
  let holdConfig: Promise<void> | null = null;
  const storage: SessionStorage = {
    loadConfig: async () => ({ baseUrl: "http://h", clientId: "cid" }),
    saveConfig: async (c) => {
      const waiting = holdConfig;
      holdConfig = null;
      if (waiting) await waiting;
      configs.push(c);
    },
    loadTokens: async () => initial,
    saveTokens: async (t) => {
      // A slow Keystore: the next write waits until the test lets it go (and then may fail).
      const waiting = holdSave;
      holdSave = null;
      if (waiting) await waiting;
      if (saveFails > 0) {
        saveFails -= 1;
        throw new Error("keystore");
      }
      saved.push(t as ReturnType<typeof tokens> | null);
    },
  };
  const bodies: string[] = [];
  // The token endpoint of the real instance (obop-oauth token.rs): the refresh token is single
  // use, every answer is a new pair (AT-n+1, RT-n+1), and a token that was used before ends the
  // whole sign-in (no grace window).
  const used = new Set<string>();
  let latest = 1;
  let revoked = false;
  const endpoint = (body: string): Response => {
    const token = new URLSearchParams(body).get("refresh_token") ?? "";
    if (revoked || used.has(token) || token !== `RT-${latest}`) {
      revoked = true;
      return new Response(JSON.stringify({ error: "invalid_grant" }), { status: 400 });
    }
    used.add(token);
    latest += 1;
    return new Response(
      JSON.stringify({
        access_token: `AT-${latest}`,
        refresh_token: `RT-${latest}`,
        expires_in: 3600,
        scope: "mileage.driver",
        device_id: "dev-1",
      }),
      { headers: { "content-type": "application/json" } },
    );
  };
  let respond: (body: string, url: string) => Response | Promise<Response> = endpoint;
  const oauth: OAuthDeps = {
    sha256: async () => new Uint8Array(32),
    randomBytes: async (n) => new Uint8Array(n),
    fetch: (async (url: string, init: RequestInit) => {
      const body = typeof init.body === "string" ? init.body : "";
      bodies.push(body);
      urls.push(url);
      return respond(body, url);
    }) as unknown as typeof fetch,
  };
  const session = createSession({ storage, oauth, now: () => clock, ...options });
  return {
    session,
    saved,
    configs,
    urls,
    bodies,
    setNow: (ms: number) => (clock = ms),
    respond: (fn: typeof respond) => (respond = fn),
    /** The real endpoint that does its work and then loses the answer. */
    endpoint,
    failSaves: (n: number) => (saveFails = n),
    /** The next write to the Keystore does not happen until the returned function is called. */
    holdNextSave: () => {
      let release: () => void = () => undefined;
      holdSave = new Promise<void>((resolve) => {
        release = resolve;
      });
      return release;
    },
    /** The next write of the config waits until the returned function is called. */
    holdNextConfig: () => {
      let release: () => void = () => undefined;
      holdConfig = new Promise<void>((resolve) => {
        release = resolve;
      });
      return release;
    },
  };
}

describe("the session", () => {
  test("a token with time left is used as it is", async () => {
    const r = rig();
    await r.session.restore();
    expect(r.session.status()).toBe("signed_in");
    expect(await r.session.accessToken()).toBe("AT-1");
    expect(r.bodies).toHaveLength(0);
  });

  test("a token that is about to end is refreshed once for calls that ask at the same time", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(95_000); // 5 seconds left, less than the 60 second skew
    const answers = await Promise.all([
      r.session.accessToken(),
      r.session.accessToken(),
      r.session.accessToken(),
    ]);
    expect(answers).toEqual(["AT-2", "AT-2", "AT-2"]);
    expect(r.bodies).toHaveLength(1);
    expect(new URLSearchParams(r.bodies[0]).get("refresh_token")).toBe("RT-1");
  });

  test("the token is refreshed from 60 seconds before it ends, not before", async () => {
    const early = rig();
    await early.session.restore();
    early.setNow(39_999); // 60.001 seconds left
    expect(await early.session.accessToken()).toBe("AT-1");
    expect(early.bodies).toHaveLength(0);

    const due = rig();
    await due.session.restore();
    due.setNow(40_000); // exactly 60 seconds left
    expect(await due.session.accessToken()).toBe("AT-2");
    expect(due.bodies).toHaveLength(1);
  });

  test("the rotated refresh token is stored before the new access token is used", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    await r.session.accessToken();
    expect(r.saved.at(-1)).toMatchObject({ refreshToken: "RT-2", accessToken: "AT-2" });
    expect(r.session.refreshToken()).toBe("RT-2");
  });

  test("the instance ending the sign-in drops the tokens and says expired", async () => {
    const r = rig();
    await r.session.restore();
    let heard = 0;
    r.session.subscribe(() => heard++);
    r.setNow(99_000);
    r.respond(() => new Response(JSON.stringify({ error: "invalid_grant" }), { status: 400 }));
    expect(await r.session.accessToken()).toBeNull();
    expect(r.session.status()).toBe("expired");
    expect(r.saved.at(-1)).toBeNull();
    expect(heard).toBe(1);
  });

  test("no connection while refreshing keeps the tokens and throws", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.respond(() => {
      throw new TypeError("Network request failed");
    });
    await expect(r.session.accessToken()).rejects.toBeInstanceOf(OAuthError);
    expect(r.session.status()).toBe("signed_in");
    expect(r.session.refreshToken()).toBe("RT-1");
    expect(r.saved).toHaveLength(0);
    // The next call tries again.
    // The request never reached the instance, so the same refresh token is still good.
    r.respond((body) => r.endpoint(body));
    expect(await r.session.accessToken()).toBe("AT-2");
    expect(r.session.status()).toBe("signed_in");
  });

  test("a refresh whose answer was lost after the instance rotated ends the sign-in at the next try (known loss mode)", async () => {
    // The instance has no grace window, so a spent refresh token that is presented again revokes
    // the whole grant. The app cannot tell this case from "never arrived"; docs/ISSUES.md L29.
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.respond((body) => {
      r.endpoint(body);
      throw new TypeError("Network request failed");
    });
    await expect(r.session.accessToken()).rejects.toBeInstanceOf(OAuthError);
    expect(r.session.status()).toBe("signed_in");
    r.respond((body) => r.endpoint(body));
    expect(await r.session.accessToken()).toBeNull();
    expect(r.session.status()).toBe("expired");
  });

  test("a failed Keystore write does not bring the spent refresh token back", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.failSaves(2); // both writes fail
    expect(await r.session.accessToken()).toBe("AT-2");
    expect(r.session.refreshToken()).toBe("RT-2");
    r.setNow(99_000 + 3_600_000); // the new access token is about to end as well
    expect(await r.session.accessToken()).toBe("AT-3");
    expect(r.session.status()).toBe("signed_in");
    expect(r.bodies.map((b) => new URLSearchParams(b).get("refresh_token"))).toEqual([
      "RT-1",
      "RT-2",
    ]);
  });

  test("one failed write is repeated once", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.failSaves(1);
    await r.session.accessToken();
    expect(r.saved.at(-1)).toMatchObject({ refreshToken: "RT-2" });
  });

  test("a client registers again when it was never used, is for another address, or is old", () => {
    const now = 1_000_000_000_000;
    const used = { baseUrl: "http://h", clientId: "c", usedAt: now - 1000 };
    expect(needsRegistration(null, "http://h", now)).toBe(true);
    expect(needsRegistration({ baseUrl: "http://h", clientId: "c" }, "http://h", now)).toBe(true);
    expect(needsRegistration(used, "http://other", now)).toBe(true);
    expect(needsRegistration(used, "http://h", now)).toBe(false);
    expect(needsRegistration({ ...used, usedAt: now - CLIENT_FRESH_MS - 1 }, "http://h", now)).toBe(
      true,
    );
    expect(needsRegistration({ ...used, usedAt: now - CLIENT_FRESH_MS + 1 }, "http://h", now)).toBe(
      false,
    );
  });

  test("only the 400 error page of the authorization page means that the instance does not know the client", () => {
    expect(clientUnknown(400)).toBe(true);
    // The sign-in page (200), a redirect to it, a server in trouble, or no answer at all: go on.
    for (const status of [200, 302, 404, 500, 503, null]) {
      expect(clientUnknown(status)).toBe(false);
    }
  });

  test("a busy server or a rate limit does not end the sign-in", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.respond(() => new Response(JSON.stringify({ error: "rate_limited" }), { status: 429 }));
    await expect(r.session.accessToken()).rejects.toMatchObject({ status: 429 });
    r.respond(() => new Response("", { status: 503 }));
    await expect(r.session.accessToken()).rejects.toMatchObject({ status: 503 });
    expect(r.session.status()).toBe("signed_in");
  });

  test("a rejected token is refreshed on the next ask", async () => {
    const r = rig();
    await r.session.restore();
    await r.session.rejected();
    expect(await r.session.accessToken()).toBe("AT-2");
  });

  /** The instance answers when the test says so: a refresh that is on its way while something else happens. */
  function held(r: ReturnType<typeof rig>) {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    r.respond(async (body) => {
      const answer = r.endpoint(body);
      await gate;
      return answer;
    });
    return release;
  }

  test("signing out while a refresh is on its way stays signed out and writes nothing", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    const release = held(r);
    const asked = r.session.accessToken();
    await Promise.resolve();
    await r.session.signOut();
    expect(r.session.status()).toBe("signed_out");
    const writesAtSignOut = r.saved.length;
    release();
    expect(await asked).toBeNull();
    // The late answer neither brought the session back nor reached the Keystore.
    expect(r.session.status()).toBe("signed_out");
    expect(r.session.refreshToken()).toBeNull();
    expect(await r.session.accessToken()).toBeNull();
    expect(r.saved).toHaveLength(writesAtSignOut);
    expect(r.saved.at(-1)).toBeNull();
  });

  test("a sign-in that finishes while an older refresh is on its way is not overwritten by it", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    const release = held(r);
    const asked = r.session.accessToken();
    await Promise.resolve();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 10_000_000));
    release();
    expect(await asked).toBeNull();
    expect(r.session.refreshToken()).toBe("RT-9");
    expect(r.saved.at(-1)).toMatchObject({ refreshToken: "RT-9" });
    expect(r.session.status()).toBe("signed_in");
  });

  test("the generation changes when the sign-in changes, and not when a token is refreshed", async () => {
    const r = rig();
    await r.session.restore();
    const first = r.session.generation();
    r.setNow(99_000);
    await r.session.accessToken();
    expect(r.session.generation()).toBe(first);
    await r.session.signOut();
    const out = r.session.generation();
    expect(out).not.toBe(first);
    await r.session.completeSignIn(tokens(5, 10_000_000));
    expect(r.session.generation()).not.toBe(out);
  });

  test("a refresh that is given up after a timeout ends the sign-in cleanly and never presents the old token again", async () => {
    const r = rig(tokens(1, 100_000), { refreshTimeoutMs: 20 });
    await r.session.restore();
    r.setNow(99_000);
    r.respond(() => new Promise<Response>(() => {}));
    expect(await r.session.accessToken()).toBeNull();
    expect(r.session.status()).toBe("expired");
    expect(r.session.endedBy()).toBe("unsure");
    expect(r.saved.at(-1)).toBeNull();
    expect(r.session.refreshToken()).toBeNull();
    // The instance is told, in the background, with the old token (it still finds the grant).
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(r.urls.at(-1)).toBe("http://h/oauth/revoke");
    // And the old token is not used for a second refresh.
    r.respond((body) => r.endpoint(body));
    expect(await r.session.accessToken()).toBeNull();
    expect(r.bodies.filter((b) => b.includes("grant_type=refresh_token"))).toHaveLength(1);
  });

  test("a client that the instance does not know is forgotten, so that the next sign-in registers again", async () => {
    const r = rig();
    await r.session.restore();
    // A client that an earlier sign-in used: the app would reuse it as it is.
    await r.session.setConfig({ baseUrl: "http://h", clientId: "cid", usedAt: Date.now() });
    expect(needsRegistration(r.session.config(), "http://h", Date.now())).toBe(false);
    r.setNow(99_000);
    r.respond(() => new Response(JSON.stringify({ error: "invalid_client" }), { status: 401 }));
    expect(await r.session.accessToken()).toBeNull();
    expect(r.session.status()).toBe("expired");
    expect(r.session.endedBy()).toBe("refused");
    expect((r.configs.at(-1) as { usedAt?: number }).usedAt).toBeUndefined();
    expect(needsRegistration(r.session.config(), "http://h", Date.now())).toBe(true);
  });

  const later = () => new Promise((resolve) => setTimeout(resolve, 15));
  const fresh = (n: number) =>
    new Response(
      JSON.stringify({
        access_token: `AT-${n}`,
        refresh_token: `RT-${n}`,
        expires_in: 3600,
        scope: "mileage.driver",
        device_id: "dev-1",
      }),
      { headers: { "content-type": "application/json" } },
    );
  const refreshWith = (body: string) => new URLSearchParams(body).get("refresh_token");
  const isRevoke = (url: string) => url.endsWith("/oauth/revoke");

  test("the write of a sign-out is not overtaken by the slow write of a refresh", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    const release = r.holdNextSave();
    const asked = r.session.accessToken();
    await later(); // the refresh is answered and its write to the Keystore is under way
    const out = r.session.signOut();
    await later();
    release();
    await out;
    await asked;
    // Written one after the other: the sign-out is the last word.
    expect(r.saved.map((t) => t?.refreshToken ?? null)).toEqual(["RT-2", null]);
    expect(r.session.status()).toBe("signed_out");
  });

  test("a refresh asked for after a sign-out and a new sign-in does not join the one of the old sign-in", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    const release = held(r);
    const stale = r.session.accessToken(); // RT-1 is on its way
    await later();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 99_500)); // about to end as well
    r.respond((body) => (refreshWith(body) === "RT-9" ? fresh(10) : r.endpoint(body)));
    expect(await r.session.accessToken()).toBe("AT-10");
    release();
    expect(await stale).toBeNull();
    expect(r.session.refreshToken()).toBe("RT-10");
  });

  test("the end of an old refresh does not free the slot of the refresh of the new sign-in", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    const gates = new Map<string, () => void>();
    r.respond(async (body) => {
      const token = refreshWith(body) ?? "";
      const answer = token === "RT-9" ? fresh(10) : r.endpoint(body);
      await new Promise<void>((resolve) => gates.set(token, resolve));
      return answer;
    });
    const stale = r.session.accessToken(); // RT-1, held
    await later();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 99_500));
    const mine = r.session.accessToken(); // RT-9, held
    await later();
    gates.get("RT-1")!();
    expect(await stale).toBeNull();
    // A third caller still shares the refresh of the new sign-in: RT-9 is presented once.
    const third = r.session.accessToken();
    await later();
    gates.get("RT-9")!();
    expect(await Promise.all([mine, third])).toEqual(["AT-10", "AT-10"]);
    expect(r.bodies.filter((b) => refreshWith(b) === "RT-9")).toHaveLength(1);
  });

  test("a refusal that comes after a sign-out does not end the next sign-in", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    let release: () => void = () => undefined;
    r.respond(async () => {
      await new Promise<void>((resolve) => (release = resolve));
      return new Response(JSON.stringify({ error: "invalid_grant" }), { status: 400 });
    });
    const stale = r.session.accessToken();
    await later();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 10_000_000));
    release();
    expect(await stale).toBeNull();
    expect(r.session.status()).toBe("signed_in");
    expect(r.session.endedBy()).toBeNull();
    expect(r.session.refreshToken()).toBe("RT-9");
    expect(r.saved.at(-1)).toMatchObject({ refreshToken: "RT-9" });
  });

  test("a refresh that times out after a sign-out does not end the next sign-in or tell the instance", async () => {
    const r = rig(tokens(1, 100_000), { refreshTimeoutMs: 40 });
    await r.session.restore();
    r.setNow(99_000);
    r.respond(() => new Promise<Response>(() => {}));
    const stale = r.session.accessToken();
    await later();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 10_000_000));
    expect(await stale).toBeNull(); // given up after 40 ms
    expect(r.session.status()).toBe("signed_in");
    expect(r.session.refreshToken()).toBe("RT-9");
    expect(r.urls.some(isRevoke)).toBe(false);
  });

  test("a failed write of a refresh is not repeated after a sign-out", async () => {
    const r = rig();
    await r.session.restore();
    r.setNow(99_000);
    r.failSaves(1);
    const release = r.holdNextSave(); // the first write waits, and fails when it is let go
    const asked = r.session.accessToken();
    await later();
    const out = r.session.signOut();
    release();
    await out;
    await asked;
    // The repeat would have come after the sign-out and written the refreshed pair over it.
    expect(r.saved.map((t) => t?.refreshToken ?? null)).toEqual([null]);
  });

  test("a sign-in that ends on its own after a timeout says so when the instance could not be told either", async () => {
    let untold = 0;
    const r = rig(tokens(1, 100_000), { refreshTimeoutMs: 20, onEndUntold: () => untold++ });
    await r.session.restore();
    r.setNow(99_000);
    // The refresh is not answered, and the revoke fails on the connection.
    r.respond((_body, url) => {
      if (isRevoke(url)) throw new TypeError("Network request failed");
      return new Promise<Response>(() => {});
    });
    expect(await r.session.accessToken()).toBeNull();
    await later();
    await later();
    expect(r.session.endedBy()).toBe("unsure");
    expect(untold).toBe(1);
  });

  test("it says nothing when the instance was told, or when a newer sign-in has begun", async () => {
    let untold = 0;
    const told = rig(tokens(1, 100_000), { refreshTimeoutMs: 20, onEndUntold: () => untold++ });
    await told.session.restore();
    told.setNow(99_000);
    told.respond((_body, url) =>
      isRevoke(url) ? new Response("{}", { status: 200 }) : new Promise<Response>(() => {}),
    );
    await told.session.accessToken();
    await later();
    await later();
    expect(untold).toBe(0);

    const newer = rig(tokens(1, 100_000), { refreshTimeoutMs: 20, onEndUntold: () => untold++ });
    await newer.session.restore();
    newer.setNow(99_000);
    let fail: () => void = () => undefined;
    newer.respond((_body, url) =>
      isRevoke(url)
        ? new Promise<Response>((_resolve, reject) => {
            fail = () => reject(new TypeError("Network request failed"));
          })
        : new Promise<Response>(() => {}),
    );
    await newer.session.accessToken();
    await later();
    await newer.session.completeSignIn(tokens(9, 10_000_000));
    fail();
    await later();
    expect(untold).toBe(0);
  });

  test("a refresh refused as invalid_client does not end a sign-in that finished while the config was written", async () => {
    const r = rig();
    await r.session.restore();
    await r.session.setConfig({ baseUrl: "http://h", clientId: "cid", usedAt: Date.now() });
    r.setNow(99_000);
    r.respond(() => new Response(JSON.stringify({ error: "invalid_client" }), { status: 401 }));
    const release = r.holdNextConfig(); // the write of the cleared config is slow
    const stale = r.session.accessToken();
    await later();
    await r.session.signOut();
    await r.session.completeSignIn(tokens(9, 10_000_000));
    release();
    expect(await stale).toBeNull();
    expect(r.session.status()).toBe("signed_in");
    expect(r.session.refreshToken()).toBe("RT-9");
  });

  test("sign out forgets the tokens and keeps the server", async () => {
    const r = rig();
    await r.session.restore();
    await r.session.signOut();
    expect(r.session.status()).toBe("signed_out");
    expect(await r.session.accessToken()).toBeNull();
    expect(r.session.config()).toEqual({ baseUrl: "http://h", clientId: "cid" });
    expect(r.saved.at(-1)).toBeNull();
  });
});

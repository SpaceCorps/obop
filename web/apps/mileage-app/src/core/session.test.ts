import { OAuthError, type OAuthDeps } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import {
  CLIENT_FRESH_MS,
  createSession,
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

function rig(initial = tokens(1, 100_000)) {
  const saved: (ReturnType<typeof tokens> | null)[] = [];
  let clock = 0;
  let saveFails = 0;
  const storage: SessionStorage = {
    loadConfig: async () => ({ baseUrl: "http://h", clientId: "cid" }),
    saveConfig: async () => {},
    loadTokens: async () => initial,
    saveTokens: async (t) => {
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
  let respond: (body: string) => Response | Promise<Response> = endpoint;
  const oauth: OAuthDeps = {
    sha256: async () => new Uint8Array(32),
    randomBytes: async (n) => new Uint8Array(n),
    fetch: (async (_url: string, init: RequestInit) => {
      const body = typeof init.body === "string" ? init.body : "";
      bodies.push(body);
      return respond(body);
    }) as unknown as typeof fetch,
  };
  const session = createSession({ storage, oauth, now: () => clock });
  return {
    session,
    saved,
    bodies,
    setNow: (ms: number) => (clock = ms),
    respond: (fn: typeof respond) => (respond = fn),
    /** The real endpoint that does its work and then loses the answer. */
    endpoint,
    failSaves: (n: number) => (saveFails = n),
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

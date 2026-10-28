import { OAuthError, type OAuthDeps } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import { createSession, type SessionStorage } from "./session.ts";

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
  const storage: SessionStorage = {
    loadConfig: async () => ({ baseUrl: "http://h", clientId: "cid" }),
    saveConfig: async () => {},
    loadTokens: async () => initial,
    saveTokens: async (t) => {
      saved.push(t as ReturnType<typeof tokens> | null);
    },
  };
  const bodies: string[] = [];
  let respond: () => Response | Promise<Response> = () =>
    new Response(
      JSON.stringify({
        access_token: "AT-2",
        refresh_token: "RT-2",
        expires_in: 3600,
        scope: "mileage.driver",
        device_id: "dev-1",
      }),
      { headers: { "content-type": "application/json" } },
    );
  const oauth: OAuthDeps = {
    sha256: async () => new Uint8Array(32),
    randomBytes: async (n) => new Uint8Array(n),
    fetch: (async (_url: string, init: RequestInit) => {
      bodies.push(typeof init.body === "string" ? init.body : "");
      return respond();
    }) as unknown as typeof fetch,
  };
  const session = createSession({ storage, oauth, now: () => clock });
  return {
    session,
    saved,
    bodies,
    setNow: (ms: number) => (clock = ms),
    respond: (fn: typeof respond) => (respond = fn),
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
    r.respond(() => new Response("{}", { headers: { "content-type": "application/json" } }));
    await expect(r.session.accessToken()).rejects.toMatchObject({ code: "invalid_response" });
    expect(r.session.status()).toBe("signed_in");
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

import { describe, expect, test, vi } from "vite-plus/test";
import {
  ObopApiError,
  ObopApprovalRequired,
  ObopNetworkError,
  createObopClient,
  isSafeToRetry,
  type ObopClientOptions,
} from "./index.ts";

type Handler = (request: Request) => Response | Promise<Response>;

/** A client whose `fetch` is a function of the test; `seen` holds every request it received and `sent` the JSON of each body. */
function setup(handler: Handler, extra: Partial<ObopClientOptions> = {}) {
  const seen: Request[] = [];
  const sent: unknown[] = [];
  const fetch = vi.fn(async (input: Request | string | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    seen.push(request);
    const text = await request.clone().text();
    sent.push(text ? JSON.parse(text) : undefined);
    return handler(request);
  });
  const client = createObopClient({
    baseUrl: "https://acme.example.com",
    getToken: () => "tok-1",
    fetch: fetch as unknown as typeof globalThis.fetch,
    ...extra,
  });
  return { client, fetch, seen, sent };
}

const json = (body: unknown, init: { status?: number; headers?: Record<string, string> } = {}) =>
  new Response(JSON.stringify(body), {
    status: init.status,
    headers: { "content-type": "application/json", ...init.headers },
  });

const rejection = async (p: Promise<unknown>): Promise<ObopApiError> => {
  try {
    await p;
  } catch (e) {
    expect(e).toBeInstanceOf(ObopApiError);
    return e as ObopApiError;
  }
  throw new Error("the call did not reject");
};

describe("the language of the app", () => {
  test("is sent as Accept-Language on every call, asked for each time", async () => {
    let language = "sv";
    const getLanguage = vi.fn(() => language);
    const { client, seen } = setup(() => json({ value: [] }), { getLanguage });
    await client.call("party", "list");
    language = "de-AT";
    await client.call("party", "list");
    await client.api.POST("/api/party/list");
    await client.getMeta().catch(() => undefined);
    expect(seen.map((r) => r.headers.get("accept-language"))).toEqual([
      "sv",
      "de-AT",
      "de-AT",
      "de-AT",
    ]);
    expect(getLanguage).toHaveBeenCalledTimes(4);
    // The token is untouched.
    expect(seen[0]!.headers.get("authorization")).toBe("Bearer tok-1");
  });

  test("may answer later, and a full Accept-Language value goes as it is", async () => {
    const { client, seen } = setup(() => json({ value: [] }), {
      getLanguage: async () => " sv-SE,sv;q=0.9,en;q=0.5 ",
    });
    await client.call("party", "list");
    expect(seen[0]!.headers.get("accept-language")).toBe("sv-SE,sv;q=0.9,en;q=0.5");
  });

  test("sends nothing when there is no language, or when asking fails", async () => {
    for (const getLanguage of [
      undefined,
      () => undefined,
      () => null,
      () => "",
      () => "   ",
      () => {
        throw new Error("no language store");
      },
      async () => {
        throw new Error("no language store");
      },
      // Not a header value: the call still goes.
      () => "sv\nx-injected: 1",
    ]) {
      const { client, seen } = setup(() => json({ value: [] }), { getLanguage });
      await expect(client.call("party", "list")).resolves.toEqual({ value: [], denied: [] });
      expect(seen[0]!.headers.has("accept-language")).toBe(false);
      expect(seen[0]!.headers.has("x-injected")).toBe(false);
    }
  });

  test("also goes as the lang parameter when the app asks for it, which outranks a cookie", async () => {
    const { client, seen } = setup(() => json({ value: [] }), {
      getLanguage: () => "sv-SE",
      languageParam: true,
    });
    await client.call("party", "list", { limit: 1 });
    await client.bulkCreate("party", [{ name: "a" }], { mode: "partial" });
    const urls = seen.map((r) => new URL(r.url));
    expect(urls.map((u) => `${u.pathname}${u.search}`)).toEqual([
      "/api/party/list?lang=sv-SE",
      "/api/party/bulk_create?lang=sv-SE",
    ]);
    // The header goes too, and the body is what the caller sent.
    expect(seen.map((r) => r.headers.get("accept-language"))).toEqual(["sv-SE", "sv-SE"]);
  });

  test("the lang parameter is left out without the option, without a tag, and for a list", async () => {
    for (const [languageParam, getLanguage] of [
      [undefined, () => "sv"],
      [false, () => "sv"],
      [true, undefined],
      [true, () => undefined],
      [true, () => "sv-SE,sv;q=0.9,en;q=0.5"],
      [true, () => "sv&x=1"],
      [true, () => "x"],
    ] as const) {
      const { client, seen } = setup(() => json({ value: [] }), { getLanguage, languageParam });
      await client.call("party", "list");
      expect(new URL(seen[0]!.url).search, `${languageParam} ${String(getLanguage)}`).toBe("");
    }
  });

  test("an error of a call with the lang parameter names the path without it", async () => {
    const { client } = setup(() => json({ error: "no" }, { status: 400 }), {
      getLanguage: () => "sv",
      languageParam: true,
    });
    const e = await rejection(client.call("party", "list"));
    expect(e.path).toBe("/api/party/list");
    expect(e.read).toBe(true);
  });

  test("does not replace a header the call carries", async () => {
    const { client, seen } = setup(() => json({ value: [] }), { getLanguage: () => "sv" });
    await client.api.POST("/api/party/list", { headers: { "accept-language": "fi" } });
    expect(seen[0]!.headers.get("accept-language")).toBe("fi");
  });

  test("the refusal of an assertion reaches the app with its key and the text as the server worded it", async () => {
    const text = "En offert behover en kund (assertion is_set(customer) failed)";
    const { client } = setup(
      () =>
        json(
          { error: text, code: "assertion_failed", hint: "Fix it.", assert_key: "needs_customer" },
          { status: 400 },
        ),
      { getLanguage: () => "sv" },
    );
    const e = await rejection(client.call("quote", "send", { id: "1" }));
    expect(e.status).toBe(400);
    expect(e.message).toBe(text);
    expect(e.code).toBe("assertion_failed");
    expect(e.assertKey).toBe("needs_customer");
    // Without a key (or with something that is not a name) there is none.
    const { client: bare } = setup(() =>
      json({ error: text, code: "assertion_failed", assert_key: 7 }, { status: 400 }),
    );
    expect((await rejection(bare.call("quote", "send", {}))).assertKey).toBeUndefined();
  });
});

describe("a call", () => {
  test("goes to the base URL with the bearer token and a JSON body", async () => {
    const { client, seen, sent } = setup(() => json({ value: [{ id: "a" }], denied: ["email"] }));
    const r = await client.call("party", "list", { limit: 2 });
    expect(r).toEqual({ value: [{ id: "a" }], denied: ["email"] });
    const req = seen[0]!;
    expect(req.method).toBe("POST");
    expect(req.url).toBe("https://acme.example.com/api/party/list");
    expect(req.headers.get("authorization")).toBe("Bearer tok-1");
    expect(req.headers.get("content-type")).toContain("application/json");
    expect(sent[0]).toEqual({ limit: 2 });
  });

  test("a trailing slash of the base URL is ignored", async () => {
    const { client, seen } = setup(() => json({ value: [] }), {
      baseUrl: "https://acme.example.com//",
    });
    await client.call("party", "list");
    expect(seen[0]!.url).toBe("https://acme.example.com/api/party/list");
  });

  test("with no input it sends an empty object", async () => {
    const { client, sent } = setup(() => json({ value: [] }));
    await client.call("party", "list");
    expect(sent[0]).toEqual({});
  });

  test("the typed openapi-fetch client is the same pipeline", async () => {
    const { client, seen } = setup(() => json({ value: [] }));
    const { data } = await client.api.POST("/api/party/list");
    expect(data).toEqual({ value: [] });
    expect(seen[0]!.headers.get("authorization")).toBe("Bearer tok-1");
  });

  test("an agent token that only proposed the step rejects with the plan", async () => {
    const { client } = setup(() =>
      json({ approval_required: true, plan_id: "plan-9" }, { status: 202 }),
    );
    const e = await rejection(client.call("party", "delete", { id: "a" }));
    expect(e).toBeInstanceOf(ObopApprovalRequired);
    expect((e as ObopApprovalRequired).planId).toBe("plan-9");
    expect(e.applied).toBe("none");
  });

  test("the global fetch is looked up at the call, so a stub set later is used", async () => {
    const client = createObopClient({ baseUrl: "https://acme.example.com", getToken: () => null });
    const stub = vi.fn(async () => json({ value: [] }));
    vi.stubGlobal("fetch", stub);
    try {
      await client.call("party", "list");
      expect(stub).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("the token", () => {
  test("is asked for at every call, so a refreshed token is used at once", async () => {
    let n = 0;
    const { client, seen } = setup(() => json({ value: [] }), { getToken: () => `tok-${++n}` });
    await client.call("party", "list");
    await client.call("party", "list");
    expect(seen.map((r) => r.headers.get("authorization"))).toEqual([
      "Bearer tok-1",
      "Bearer tok-2",
    ]);
  });

  test("may be answered later", async () => {
    const { client, seen } = setup(() => json({ value: [] }), {
      getToken: async () => {
        await Promise.resolve();
        return "late";
      },
    });
    await client.call("party", "list");
    expect(seen[0]!.headers.get("authorization")).toBe("Bearer late");
  });

  test.each([null, undefined, ""])("%j sends no Authorization header", async (token) => {
    const { client, seen } = setup(() => json({ value: [] }), { getToken: () => token });
    await client.call("party", "list");
    expect(seen[0]!.headers.has("authorization")).toBe(false);
  });

  test("an error of getToken rejects the call and nothing is sent", async () => {
    const { client, fetch } = setup(() => json({ value: [] }), {
      getToken: () => {
        throw new Error("the secure store is locked");
      },
    });
    await expect(client.call("party", "list")).rejects.toThrow("the secure store is locked");
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe("failures become ObopApiError", () => {
  // [status, body, extra headers, what the error says]
  const table: [number, unknown, Record<string, string>, Record<string, unknown>][] = [
    [
      400,
      { error: "name is required" },
      {},
      { status: 400, message: "name is required", code: undefined },
    ],
    [
      400,
      { error: "bad cursor", code: "cursor_invalid", hint: "Pass the cursor unchanged." },
      {},
      { status: 400, code: "cursor_invalid", hint: "Pass the cursor unchanged." },
    ],
    [
      403,
      { error: "set up two-factor", code: "mfa_enrolment_required", hint: "Open Security." },
      {},
      { status: 403, code: "mfa_enrolment_required" },
    ],
    [404, { error: "not found" }, {}, { status: 404, message: "not found" }],
    [
      409,
      {
        error: "the record changed",
        code: "state_conflict",
        hint: "Reload the record and try again.",
      },
      {},
      { status: 409, code: "state_conflict" },
    ],
    [
      412,
      { error: "stale", code: "precondition_failed", hint: "Read the record again." },
      { etag: '"7"' },
      { status: 412, code: "precondition_failed" },
    ],
    [410, { error: "old", code: "cursor_expired" }, {}, { status: 410, code: "cursor_expired" }],
    [
      429,
      {
        error: "Too many wrong codes.",
        code: "rate_limited",
        hint: "Wait a minute and try again.",
      },
      { "retry-after": "7" },
      { status: 429, code: "rate_limited", retryAfterSeconds: 7 },
    ],
    [
      503,
      { error: "storage_busy", code: "storage_busy", hint: "Retry.", applied: "none" },
      { "retry-after": "1", "x-obop-applied": "none" },
      { status: 503, code: "storage_busy", applied: "none", retryAfterSeconds: 1 },
    ],
    [
      503,
      { error: "storage_unavailable", code: "storage_unavailable", applied: "unknown" },
      { "retry-after": "5", "x-obop-applied": "unknown" },
      { status: 503, code: "storage_unavailable", applied: "unknown", retryAfterSeconds: 5 },
    ],
    [
      503,
      { error: "storage_timeout", code: "storage_timeout", applied: "partial" },
      { "retry-after": "5", "x-obop-applied": "partial" },
      { status: 503, code: "storage_timeout", applied: "partial", retryAfterSeconds: 5 },
    ],
    [
      503,
      { error: "the server is busy, try again", code: "unavailable" },
      { "retry-after": "1" },
      { status: 503, code: "unavailable", applied: undefined, retryAfterSeconds: 1 },
    ],
  ];

  test.each(table)("%i %j", async (status, body, headers, expected) => {
    const { client } = setup(() => json(body, { status, headers }));
    const e = await rejection(client.call("party", "create", { name: "Ada" }));
    expect(e).toMatchObject(expected);
    expect(e).not.toBeInstanceOf(ObopNetworkError);
    expect(e.method).toBe("POST");
    expect(e.path).toBe("/api/party/create");
    expect(e.read).toBe(false);
  });

  test("an empty 401 says what the status says", async () => {
    const { client } = setup(() => new Response(null, { status: 401, statusText: "Unauthorized" }));
    const e = await rejection(client.call("party", "list"));
    expect(e).toMatchObject({ status: 401, message: "Unauthorized", code: undefined });
    expect(e.read).toBe(true);
  });

  test("a gateway page that is not JSON is still one error", async () => {
    const { client } = setup(
      () => new Response("<html>Bad gateway</html>", { status: 502, statusText: "Bad Gateway" }),
    );
    const e = await rejection(client.call("party", "list"));
    expect(e).toMatchObject({
      status: 502,
      message: "Bad Gateway",
      body: "<html>Bad gateway</html>",
    });
    expect(isSafeToRetry(e)).toBe(true);
  });

  test("a failed GET of the typed client rejects too", async () => {
    const { client } = setup(() => json({ error: "nope" }, { status: 500 }));
    const e = await rejection(client.getMeta());
    expect(e).toMatchObject({ status: 500, method: "GET", path: "/api/meta", read: true });
  });

  test("the error of a 503 tells whether the call may be repeated", async () => {
    const { client } = setup(() =>
      json({ error: "x", code: "storage_busy", applied: "none" }, { status: 503 }),
    );
    const e = await rejection(client.call("party", "create", { name: "Ada" }));
    expect(isSafeToRetry(e)).toBe(true);
    const { client: other } = setup(() =>
      json({ error: "x", code: "storage_unavailable", applied: "unknown" }, { status: 503 }),
    );
    const u = await rejection(other.call("party", "create", { name: "Ada" }));
    expect(isSafeToRetry(u)).toBe(false);
    expect(isSafeToRetry(u, { idempotent: true })).toBe(true);
  });
});

describe("a lost connection", () => {
  const down = () => {
    throw new TypeError("fetch failed");
  };

  test("is a network error whose outcome is unknown for a write", async () => {
    const { client } = setup(down);
    const e = await rejection(client.call("party", "create", { name: "Ada" }));
    expect(e).toBeInstanceOf(ObopNetworkError);
    expect(e).toMatchObject({ status: 0, code: "network_error", applied: "unknown", read: false });
    expect((e.cause as Error).message).toBe("fetch failed");
    expect(isSafeToRetry(e)).toBe(false);
  });

  test("changed nothing for a read, which can be repeated", async () => {
    const { client } = setup(down);
    const e = await rejection(client.call("party", "search", {}));
    expect(e).toBeInstanceOf(ObopNetworkError);
    expect(e).toMatchObject({ applied: "none", read: true });
    expect(isSafeToRetry(e)).toBe(true);
  });

  test("a call the caller cancelled is not reported as a network failure", async () => {
    const { client } = setup(() => {
      throw new DOMException("The operation was aborted", "AbortError");
    });
    const caught = await client.call("party", "list").catch((e: unknown) => e);
    expect(caught).toBeInstanceOf(Error);
    expect(caught).not.toBeInstanceOf(ObopApiError);
    expect((caught as Error).name).toBe("AbortError");
  });
});

describe("onUnauthorized", () => {
  test("is called once for a 401, the call rejects, and nothing is sent again", async () => {
    const onUnauthorized = vi.fn();
    const { client, fetch } = setup(() => new Response(null, { status: 401 }), { onUnauthorized });
    const e = await rejection(client.call("party", "list"));
    expect(e.status).toBe(401);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  test("is awaited before the call rejects", async () => {
    const order: string[] = [];
    const { client } = setup(() => new Response(null, { status: 401 }), {
      onUnauthorized: async () => {
        await Promise.resolve();
        order.push("signed out");
      },
    });
    await client.call("party", "list").catch(() => order.push("rejected"));
    expect(order).toEqual(["signed out", "rejected"]);
  });

  test("401s that arrive together share one run", async () => {
    const onUnauthorized = vi.fn(async () => {
      await Promise.resolve();
    });
    const { client } = setup(() => new Response(null, { status: 401 }), { onUnauthorized });
    const all = await Promise.allSettled([
      client.call("party", "list"),
      client.call("product", "list"),
      client.call("lead", "list"),
    ]);
    expect(all.every((r) => r.status === "rejected")).toBe(true);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  test("a later 401 calls it again", async () => {
    const onUnauthorized = vi.fn();
    const { client } = setup(() => new Response(null, { status: 401 }), { onUnauthorized });
    await rejection(client.call("party", "list"));
    await rejection(client.call("party", "list"));
    expect(onUnauthorized).toHaveBeenCalledTimes(2);
  });

  test("is not called for a call that sent no token (a wrong password is a 401 too)", async () => {
    const onUnauthorized = vi.fn();
    const { client } = setup(() => json({ error: "wrong password" }, { status: 401 }), {
      onUnauthorized,
      getToken: () => null,
    });
    await rejection(client.call("auth", "login", { username: "a", password: "b" }));
    expect(onUnauthorized).not.toHaveBeenCalled();
  });

  test("is not called for other statuses", async () => {
    const onUnauthorized = vi.fn();
    const { client } = setup(() => json({ error: "no" }, { status: 403 }), { onUnauthorized });
    await rejection(client.call("party", "list"));
    expect(onUnauthorized).not.toHaveBeenCalled();
  });

  test("an error in the handler does not hide the 401", async () => {
    const { client } = setup(() => new Response(null, { status: 401 }), {
      onUnauthorized: () => {
        throw new Error("handler broke");
      },
    });
    const e = await rejection(client.call("party", "list"));
    expect(e.status).toBe(401);
  });
});

describe("bulkCreate", () => {
  /** A stand-in for the instance: remembers the first answer of each idempotency key. */
  function instance() {
    const stored = new Map<string, { fingerprint: string; report: Record<string, unknown> }>();
    let created = 0;
    const handler: Handler = async (request) => {
      const body = (await request.json()) as {
        rows: unknown[];
        mode?: string;
        dry_run?: boolean;
        idempotency_key?: string;
      };
      const fingerprint = JSON.stringify([body.rows, body.mode ?? "all_or_nothing"]);
      const key = body.idempotency_key;
      if (key && !body.dry_run) {
        const first = stored.get(key);
        if (first && first.fingerprint !== fingerprint) {
          return json(
            { error: "this idempotency_key was already used for a different request" },
            { status: 400 },
          );
        }
        if (first) return json({ value: { ...first.report, replayed: true }, denied: [] });
      }
      const report: Record<string, unknown> = {
        ok: true,
        dry_run: body.dry_run === true,
        mode: body.mode ?? "all_or_nothing",
        total: body.rows.length,
        // As the server does: a dry run counts `valid` rows and has no ids yet.
        ...(body.dry_run
          ? { valid: body.rows.length }
          : { created: body.rows.length, ids: body.rows.map(() => `id-${++created}`) }),
        failed: 0,
        errors: [],
        replayed: false,
      };
      if (key && !body.dry_run) stored.set(key, { fingerprint, report });
      return json({ value: report, denied: [] });
    };
    return handler;
  }

  test("sends the rows with the idempotency key, mode and dry_run, and returns the report", async () => {
    const { client, seen, sent } = setup(instance());
    const report = await client.bulkCreate("party", [{ name: "Ada" }, { name: "Grace" }], {
      idempotencyKey: "trip:0192f3a4",
      mode: "partial",
      dryRun: false,
    });
    expect(report).toMatchObject({ ok: true, total: 2, created: 2, replayed: false });
    expect(seen[0]!.url).toBe("https://acme.example.com/api/party/bulk_create");
    expect(sent[0]).toEqual({
      rows: [{ name: "Ada" }, { name: "Grace" }],
      mode: "partial",
      dry_run: false,
      idempotency_key: "trip:0192f3a4",
    });
  });

  test("sends only what was given", async () => {
    const { client, sent } = setup(instance());
    await client.bulkCreate("party", [{ name: "Ada" }]);
    expect(sent[0]).toEqual({ rows: [{ name: "Ada" }] });
  });

  test("a repeat with the same key and rows returns the first answer, marked replayed", async () => {
    const { client } = setup(instance());
    const rows = [{ name: "Ada" }];
    const first = await client.bulkCreate("party", rows, { idempotencyKey: "k-1" });
    const again = await client.bulkCreate("party", rows, { idempotencyKey: "k-1" });
    expect(first.replayed).toBe(false);
    expect(again.replayed).toBe(true);
    expect(again.ids).toEqual(first.ids);
  });

  test("the same key with other rows is refused", async () => {
    const { client } = setup(instance());
    await client.bulkCreate("party", [{ name: "Ada" }], { idempotencyKey: "k-2" });
    const e = await rejection(
      client.bulkCreate("party", [{ name: "Grace" }], { idempotencyKey: "k-2" }),
    );
    expect(e.status).toBe(400);
    expect(e.message).toContain("idempotency_key");
  });

  test("a dry run reports what would be created and stores nothing", async () => {
    const { client } = setup(instance());
    const report = await client.bulkCreate("party", [{ name: "Ada" }], { dryRun: true });
    expect(report).toMatchObject({ dry_run: true, valid: 1, failed: 0 });
    expect(report.created).toBeUndefined();
  });

  test("a model the schema does not know takes untyped rows", async () => {
    const { client, seen } = setup(instance());
    await client.bulkCreate("mileage_trip", [{ client_trip_id: "t-1", anything: 1 }]);
    expect(seen[0]!.url).toBe("https://acme.example.com/api/mileage_trip/bulk_create");
  });

  test.each(["", "has space", "ünïcode", "x".repeat(129), "tab\tkey"])(
    "the key %j is refused before anything is sent",
    async (key) => {
      const { client, fetch } = setup(instance());
      await expect(
        client.bulkCreate("party", [{ name: "Ada" }], { idempotencyKey: key }),
      ).rejects.toThrow(RangeError);
      expect(fetch).not.toHaveBeenCalled();
    },
  );

  test("a key of 128 visible characters is sent", async () => {
    const { client, fetch } = setup(instance());
    await client.bulkCreate("party", [{ name: "Ada" }], { idempotencyKey: "k".repeat(128) });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  test("a 503 that says unknown can be repeated because the call carries a key", async () => {
    let calls = 0;
    const { client } = setup((request) => {
      calls += 1;
      return calls === 1
        ? json(
            { error: "storage_unavailable", code: "storage_unavailable", applied: "unknown" },
            { status: 503, headers: { "retry-after": "5" } },
          )
        : instance()(request);
    });
    const rows = [{ name: "Ada" }];
    const e = await rejection(client.bulkCreate("party", rows, { idempotencyKey: "k-3" }));
    expect(isSafeToRetry(e)).toBe(false);
    expect(isSafeToRetry(e, { idempotent: true })).toBe(true);
    const report = await client.bulkCreate("party", rows, { idempotencyKey: "k-3" });
    expect(report.created).toBe(1);
  });
});

describe("getMeta", () => {
  test("reads GET /api/meta with the token", async () => {
    const { client, seen } = setup(() =>
      json({
        core_version: "0.2.0",
        time: { zone: "UTC" },
        modules: [{ id: "base", name: "Base", models: [] }],
      }),
    );
    const meta = await client.getMeta();
    expect(meta.core_version).toBe("0.2.0");
    expect(meta.modules.map((m) => m.id)).toEqual(["base"]);
    expect(seen[0]!.method).toBe("GET");
    expect(seen[0]!.url).toBe("https://acme.example.com/api/meta");
    expect(seen[0]!.headers.get("authorization")).toBe("Bearer tok-1");
  });
});

describe("onUnauthorized that calls the client itself", () => {
  const deadlock = { timeout: 3000 };

  test("does not wait for itself when its own call gets a 401 too", deadlock, async () => {
    let handler = 0;
    let nested: unknown;
    let self: ObopClientRef = undefined;
    const { client, fetch } = setup(() => new Response(null, { status: 401 }), {
      onUnauthorized: async () => {
        handler += 1;
        // A refresh through the same client, still with the expired token: it is a 401 as well.
        nested = await self!.call("auth", "refresh", {}).catch((e: unknown) => e);
      },
    });
    self = client;
    const e = await rejection(client.call("party", "list"));
    expect(e.status).toBe(401);
    expect(handler).toBe(1);
    expect(nested).toBeInstanceOf(ObopApiError);
    expect((nested as ObopApiError).status).toBe(401);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  test("runs once for a burst of 401s although its own call gets one", deadlock, async () => {
    let handler = 0;
    let self: ObopClientRef = undefined;
    const { client, fetch } = setup(() => new Response(null, { status: 401 }), {
      onUnauthorized: async () => {
        handler += 1;
        await self!.call("auth", "refresh", {}).catch(() => undefined);
      },
    });
    self = client;
    const all = await Promise.allSettled([
      client.call("party", "list"),
      client.call("product", "list"),
      client.call("lead", "list"),
    ]);
    expect(all.every((r) => r.status === "rejected")).toBe(true);
    expect(handler).toBe(1);
    expect(fetch).toHaveBeenCalledTimes(4);
  });

  test("a refresh that works lets the next call through with the new token", deadlock, async () => {
    let token = "old";
    let self: ObopClientRef = undefined;
    const { client } = setup(
      (request) =>
        request.headers.get("authorization") === "Bearer old"
          ? request.url.endsWith("/api/auth/refresh")
            ? json({ value: { token: "new" } })
            : new Response(null, { status: 401 })
          : json({ value: [{ id: "a" }] }),
      {
        getToken: () => token,
        onUnauthorized: async () => {
          // The refresh call is sent with the old token and is answered; it is not a 401.
          const r = await self!.call<{ token: string }>("auth", "refresh", {});
          token = r.value.token;
        },
      },
    );
    self = client;
    expect((await rejection(client.call("party", "list"))).status).toBe(401);
    expect((await client.call("party", "list")).value).toEqual([{ id: "a" }]);
  });
});

type ObopClientRef = ReturnType<typeof createObopClient> | undefined;

describe("an answer that is not what an instance sends", () => {
  const html = () =>
    new Response("<!doctype html><title>Welcome</title>", {
      status: 200,
      headers: { "content-type": "text/html" },
    });

  test("a 200 page rejects call with invalid_response, not a SyntaxError", async () => {
    const { client } = setup(html);
    const e = await rejection(client.call("party", "list"));
    expect(e).toMatchObject({ status: 200, code: "invalid_response", path: "/api/party/list" });
    expect(String(e.body)).toContain("<title>Welcome</title>");
    expect(isSafeToRetry(e)).toBe(false);
  });

  test("a 200 page rejects getMeta with invalid_response", async () => {
    const { client } = setup(html);
    const e = await rejection(client.getMeta());
    expect(e).toMatchObject({ status: 200, code: "invalid_response", method: "GET" });
  });

  test("the typed client is covered too", async () => {
    const { client } = setup(html);
    const e = await rejection(client.api.POST("/api/party/list"));
    expect(e.code).toBe("invalid_response");
  });

  test("so is JSON that is cut off", async () => {
    const { client } = setup(
      () =>
        new Response('{"value": [{"id": "a"', {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    );
    const e = await rejection(client.call("party", "list"));
    expect(e).toMatchObject({ status: 200, code: "invalid_response" });
  });

  test("a write that was answered with a page is not repeated", async () => {
    const { client } = setup(html);
    const e = await rejection(
      client.bulkCreate("party", [{ name: "Ada" }], { idempotencyKey: "k" }),
    );
    expect(e.code).toBe("invalid_response");
    expect(isSafeToRetry(e, { idempotent: true })).toBe(false);
  });

  test("JSON of another shape is invalid too: a list where an object is, no modules in the metadata", async () => {
    const listed = setup(() => json([1, 2, 3]));
    expect((await rejection(listed.client.call("party", "list"))).code).toBe("invalid_response");
    const other = setup(() => json({ hello: "world" }));
    expect((await rejection(other.client.getMeta())).code).toBe("invalid_response");
    const nothing = setup(() => new Response("null", { status: 200 }));
    expect((await rejection(nothing.client.getMeta())).code).toBe("invalid_response");
  });

  test("an empty success is not an error for a command", async () => {
    const { client } = setup(() => new Response(null, { status: 200 }));
    expect(await client.call("party", "ping")).toEqual({ value: undefined, denied: [] });
  });

  test("a connection that breaks while the body is read is a network error", async () => {
    const broken = () =>
      new Response(
        new ReadableStream({
          start(controller) {
            controller.error(new TypeError("terminated"));
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    const { client } = setup(broken);
    const read = await rejection(client.call("party", "list"));
    expect(read).toBeInstanceOf(ObopNetworkError);
    expect(read.applied).toBe("none");
    const write = await rejection(client.call("party", "create", { name: "Ada" }));
    expect(write).toBeInstanceOf(ObopNetworkError);
    expect(write.applied).toBe("unknown");
  });

  test("a download is not parsed as JSON", async () => {
    const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
    const { client } = setup(
      () => new Response(bytes, { status: 200, headers: { "content-type": "image/png" } }),
    );
    const { data } = await client.api.GET("/api/blobs/{id}", {
      params: { path: { id: "a".repeat(64) } },
      parseAs: "blob",
    });
    expect((data as Blob).size).toBe(8);
  });
});

import { describe, expect, test } from "vite-plus/test";
import {
  ObopApiError,
  ObopApprovalRequired,
  ObopNetworkError,
  isReadCall,
  isSafeToRetry,
  parseApplied,
  parseRetryAfter,
  retryDelayMs,
} from "./errors.ts";

const err = (
  status: number,
  extra: { applied?: "none" | "unknown" | "partial"; read?: boolean } = {},
) => new ObopApiError({ status, message: `status ${status}`, ...extra });

describe("isSafeToRetry (the D74 rule of docs/DEPLOY.md)", () => {
  // [description, error, idempotent, expected]
  const table: [string, ObopApiError, boolean | undefined, boolean][] = [
    ["503 applied none repeats a plain write", err(503, { applied: "none" }), false, true],
    ["503 applied none repeats a read", err(503, { applied: "none", read: true }), undefined, true],
    [
      "503 applied unknown does not repeat a plain write",
      err(503, { applied: "unknown" }),
      false,
      false,
    ],
    [
      "503 applied unknown repeats a write that has a key",
      err(503, { applied: "unknown" }),
      true,
      true,
    ],
    [
      "503 applied unknown repeats a read",
      err(503, { applied: "unknown", read: true }),
      undefined,
      true,
    ],
    [
      "503 applied partial does not repeat a plain write",
      err(503, { applied: "partial" }),
      false,
      false,
    ],
    [
      "503 applied partial repeats a write that has a key",
      err(503, { applied: "partial" }),
      true,
      true,
    ],
    [
      "503 applied partial repeats a read",
      err(503, { applied: "partial", read: true }),
      undefined,
      true,
    ],
    ["503 without the word is treated as unknown (a write)", err(503), false, false],
    ["503 without the word repeats a read", err(503, { read: true }), undefined, true],
    ["502 from a gateway is unknown (a write)", err(502), false, false],
    ["502 from a gateway repeats a keyed write", err(502), true, true],
    ["500 is unknown (a write)", err(500), false, false],
    ["429 is repeated after Retry-After, plain write or not", err(429), false, true],
    ["429 on a read is repeated", err(429, { read: true }), undefined, true],
    ["400 never", err(400, { read: true }), true, false],
    ["401 never", err(401), true, false],
    ["403 never", err(403), true, false],
    ["404 never", err(404, { read: true }), true, false],
    ["409 never", err(409), true, false],
    ["410 never", err(410, { read: true }), true, false],
    ["412 never", err(412), true, false],
    ["a 2xx is not a failure to repeat", err(202), true, false],
  ];
  test.each(table)("%s", (_name, error, idempotent, expected) => {
    expect(isSafeToRetry(error, idempotent === undefined ? {} : { idempotent })).toBe(expected);
  });

  test("a lost connection: applied is none for a read and unknown for a write", () => {
    const read = new ObopNetworkError(new TypeError("fetch failed"), {
      method: "POST",
      path: "/api/party/list",
      read: true,
    });
    const write = new ObopNetworkError(new TypeError("fetch failed"), {
      method: "POST",
      path: "/api/party/create",
      read: false,
    });
    expect([read.status, read.code, read.applied]).toEqual([0, "network_error", "none"]);
    expect([write.status, write.code, write.applied]).toEqual([0, "network_error", "unknown"]);
    expect(isSafeToRetry(read)).toBe(true);
    expect(isSafeToRetry(write)).toBe(false);
    expect(isSafeToRetry(write, { idempotent: true })).toBe(true);
    expect(write.cause).toBeInstanceOf(TypeError);
  });

  test("whatever is not an ObopApiError is not safe to repeat", () => {
    expect(isSafeToRetry(new Error("boom"), { idempotent: true })).toBe(false);
    expect(isSafeToRetry(undefined)).toBe(false);
    expect(isSafeToRetry("503")).toBe(false);
  });

  test("an approval that is still needed is not a failure to repeat", () => {
    expect(isSafeToRetry(new ObopApprovalRequired("plan-1"), { idempotent: true })).toBe(false);
  });
});

describe("retryDelayMs", () => {
  test("is what the server asked for, else the fallback", () => {
    expect(
      retryDelayMs(new ObopApiError({ status: 503, message: "m", retryAfterSeconds: 5 })),
    ).toBe(5000);
    expect(
      retryDelayMs(new ObopApiError({ status: 503, message: "m", retryAfterSeconds: 0 })),
    ).toBe(0);
    expect(retryDelayMs(err(503))).toBe(1000);
    expect(retryDelayMs(err(503), 250)).toBe(250);
  });
});

describe("parsing", () => {
  test("applied is one of the three words", () => {
    expect(parseApplied("none")).toBe("none");
    expect(parseApplied("unknown")).toBe("unknown");
    expect(parseApplied("partial")).toBe("partial");
    expect(parseApplied("all")).toBeUndefined();
    expect(parseApplied(null)).toBeUndefined();
    expect(parseApplied(3)).toBeUndefined();
  });

  test("Retry-After is whole seconds or an HTTP date", () => {
    expect(parseRetryAfter("5")).toBe(5);
    expect(parseRetryAfter(" 12 ")).toBe(12);
    expect(parseRetryAfter(null)).toBeUndefined();
    expect(parseRetryAfter("soon")).toBeUndefined();
    const now = Date.parse("2026-10-27T12:00:00Z");
    expect(parseRetryAfter("Tue, 27 Oct 2026 12:00:30 GMT", now)).toBe(30);
    expect(parseRetryAfter("Tue, 27 Oct 2026 11:00:00 GMT", now)).toBe(0);
  });

  test("a read is a GET or a get, list or search of a model", () => {
    expect(isReadCall("GET", "/api/meta")).toBe(true);
    expect(isReadCall("POST", "/api/party/list")).toBe(true);
    expect(isReadCall("POST", "/api/party/search")).toBe(true);
    expect(isReadCall("post", "/api/party/get")).toBe(true);
    expect(isReadCall("POST", "/api/party/create")).toBe(false);
    expect(isReadCall("POST", "/api/party/bulk_create")).toBe(false);
    expect(isReadCall("POST", "/api/sale_order/confirm")).toBe(false);
    expect(isReadCall("POST", "/api/approve/{plan_id}")).toBe(false);
    expect(isReadCall("POST", "/api/auth/login")).toBe(false);
  });
});

describe("ObopApiError.fromResponse", () => {
  test("reads the message, code, hint and applied from the JSON body", async () => {
    const e = await ObopApiError.fromResponse(
      new Response(
        JSON.stringify({
          error: "storage_busy",
          code: "storage_busy",
          hint: "Retry.",
          applied: "none",
        }),
        { status: 503, headers: { "retry-after": "1" } },
      ),
      { method: "POST", path: "/api/party/create", read: false },
    );
    expect(e).toBeInstanceOf(ObopApiError);
    expect(e).toBeInstanceOf(Error);
    expect(e.name).toBe("ObopApiError");
    expect(e).toMatchObject({
      status: 503,
      message: "storage_busy",
      code: "storage_busy",
      hint: "Retry.",
      applied: "none",
      retryAfterSeconds: 1,
      method: "POST",
      path: "/api/party/create",
      read: false,
    });
  });

  test("the X-Obop-Applied header is used when the body does not say", async () => {
    const e = await ObopApiError.fromResponse(
      new Response("{}", { status: 503, headers: { "x-obop-applied": "partial" } }),
    );
    expect(e.applied).toBe("partial");
  });

  test("the body wins over a header that disagrees", async () => {
    const e = await ObopApiError.fromResponse(
      new Response(JSON.stringify({ applied: "none" }), {
        status: 503,
        headers: { "x-obop-applied": "unknown" },
      }),
    );
    expect(e.applied).toBe("none");
  });

  test("an answer that is not JSON keeps its text and still has a message", async () => {
    const e = await ObopApiError.fromResponse(
      new Response("<html>Bad gateway</html>", { status: 502, statusText: "Bad Gateway" }),
    );
    expect(e.status).toBe(502);
    expect(e.message).toBe("Bad Gateway");
    expect(e.body).toBe("<html>Bad gateway</html>");
    expect(e.code).toBeUndefined();
    expect(e.applied).toBeUndefined();
  });

  test("an empty answer without a status text says the status", async () => {
    const e = await ObopApiError.fromResponse(new Response(null, { status: 401 }));
    expect(e.message).toBe("The server answered 401");
  });
});

import { ObopApiError } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import { classify } from "./engine.ts";
import { apiError, networkError } from "../test-support/fake-server.ts";

/**
 * How each kind of failure is read: repeat later (offline, unavailable, rate limited), repeat the
 * end (incomplete), sign in again (signed out), or stop until the driver acts (refused). The
 * choice between "repeat" and "stuck" is the one that matters, so every branch is pinned.
 */
describe("what a failed call means for the outbox", () => {
  test("no connection, a timeout or a broken call is repeated later", () => {
    expect(classify(networkError())).toEqual({ kind: "offline" });
    expect(classify(new DOMException("The operation was aborted", "AbortError"))).toEqual({
      kind: "offline",
    });
    expect(classify(new TypeError("boom"))).toEqual({ kind: "offline" });
  });

  test("401 is a sign-in that ended", () => {
    expect(classify(apiError(401))).toEqual({ kind: "signed_out" });
  });

  test("429 waits as long as the server says, a minute otherwise", () => {
    expect(classify(apiError(429, { retryAfter: 3 }))).toEqual({
      kind: "rate_limited",
      waitMs: 3000,
    });
    expect(classify(apiError(429))).toEqual({ kind: "rate_limited", waitMs: 60_000 });
  });

  test.each([500, 502, 503, 504])("%i is repeated later", (status) => {
    expect(classify(apiError(status))).toEqual({ kind: "unavailable", waitMs: null });
    expect(classify(apiError(status, { applied: "none" }))).toEqual({
      kind: "unavailable",
      waitMs: null,
    });
    expect(classify(apiError(status, { applied: "unknown" }))).toEqual({
      kind: "unavailable",
      waitMs: null,
    });
  });

  test("a 503 follows the wait the server asks for", () => {
    expect(classify(apiError(503, { retryAfter: 7 }))).toEqual({
      kind: "unavailable",
      waitMs: 7000,
    });
  });

  test("a 503 that applied part of a batch is not repeated blindly", () => {
    expect(classify(apiError(503, { applied: "partial" }))).toMatchObject({
      kind: "refused",
      status: 503,
    });
  });

  test("a request timeout (408) and a proxy or portal answering instead are repeated later", () => {
    expect(classify(apiError(408))).toEqual({ kind: "unavailable", waitMs: null });
    expect(classify(apiError(200, { code: "invalid_response" }))).toEqual({
      kind: "unavailable",
      waitMs: null,
    });
    expect(classify(apiError(502, { code: "invalid_response" }))).toEqual({
      kind: "unavailable",
      waitMs: null,
    });
  });

  test("an answer that is an obop answer but not the expected one is a refusal", () => {
    expect(classify(apiError(200, { code: "unexpected_answer" }))).toMatchObject({
      kind: "refused",
      code: "unexpected_answer",
    });
  });

  test("409 incomplete repeats the end; any other 409 is a refusal", () => {
    expect(classify(apiError(409, { code: "incomplete", message: "2 points missing" }))).toEqual({
      kind: "incomplete",
      waitMs: null,
      message: "2 points missing",
    });
    expect(classify(apiError(409, { code: "incomplete", retryAfter: 4 }))).toMatchObject({
      kind: "incomplete",
      waitMs: 4000,
    });
    expect(classify(apiError(409, { code: "state_conflict" }))).toMatchObject({
      kind: "refused",
      code: "state_conflict",
    });
  });

  test.each([400, 403, 404, 422])("%i is a refusal that waits for the driver", (status) => {
    expect(classify(apiError(status, { message: "no" }))).toEqual({
      kind: "refused",
      status,
      code: "bad_request",
      message: "no",
    });
  });

  test("a refusal keeps the code and the hint of the server", () => {
    const error = new ObopApiError({
      status: 403,
      code: "forbidden",
      message: "not allowed",
      hint: "ask an administrator",
    });
    expect(classify(error)).toEqual({
      kind: "refused",
      status: 403,
      code: "forbidden",
      message: "not allowed ask an administrator",
    });
  });
});

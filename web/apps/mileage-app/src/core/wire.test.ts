import { createObopClient } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import { createTripApi } from "./api.ts";
import { createEngine } from "./engine.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const T0 = Date.parse("2026-10-15T09:00:00Z");

/** The engine over the real client and trip calls, with a fetch that answers as a 503 of the server does. */
async function overTheWire(answer: () => Response) {
  const db = memoryDb();
  await migrate(db);
  const store = createSqlStore(db);
  const fetch = async () => answer();
  const client = createObopClient({
    baseUrl: "http://10.0.2.2:8080",
    getToken: () => "tok",
    fetch: fetch as typeof globalThis.fetch,
  });
  const engine = createEngine({
    store,
    api: createTripApi(client, { timeoutMs: 5000 }),
    backoff: { baseMs: 1000, maxMs: 8000 },
  });
  await store.startTrip({
    clientTripId: "c1",
    purpose: "business",
    startedAt: new Date(T0).toISOString(),
    nowMs: T0,
  });
  await store.stopTrip("c1", new Date(T0 + 10_000).toISOString());
  return { store, engine };
}

const json = (status: number, body: object, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });

describe("what the server says on the wire reaches the outbox", () => {
  test("a 503 with Retry-After and X-Obop-Applied: none is waited out as the server asks", async () => {
    const { engine, store } = await overTheWire(() =>
      json(503, { error: "busy" }, { "retry-after": "3", "x-obop-applied": "none" }),
    );
    expect(await engine.drain()).toMatchObject({ state: "unavailable", retryInMs: 3000 });
    expect((await store.getTrip("c1"))!.problem).toBeNull();
  });

  test("a 429 with Retry-After waits as long as the server says", async () => {
    const { engine } = await overTheWire(() =>
      json(429, { error: "slow down", code: "rate_limited" }, { "retry-after": "17" }),
    );
    expect(await engine.drain()).toMatchObject({ state: "rate_limited", retryInMs: 17_000 });
  });

  test("a portal that answers 200 with a web page is a connection problem, not a refusal", async () => {
    const { engine, store } = await overTheWire(
      () =>
        new Response("<html>Sign in to the Wi-Fi</html>", {
          headers: { "content-type": "text/html" },
        }),
    );
    const report = await engine.drain();
    expect(report.state).toBe("unavailable");
    expect(report.problems).toBe(0);
    expect((await store.getTrip("c1"))!.problem).toBeNull();
  });

  test("a 400 is a refusal that waits for the driver", async () => {
    const { engine, store } = await overTheWire(() => json(400, { error: "no" }));
    const report = await engine.drain();
    expect(report.problems).toBe(1);
    expect((await store.getTrip("c1"))!.problem).toMatchObject({
      step: "create",
      code: "bad_request",
    });
  });
});

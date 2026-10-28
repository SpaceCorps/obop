import { createObopClient, ObopApiError } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import { createTripApi } from "./api.ts";

interface Sent {
  path: string;
  body: Record<string, unknown>;
  auth: string | null;
}

/** An instance that answers each path with the value a test gives it. */
function instance(answers: Record<string, object | (() => Response)>) {
  const sent: Sent[] = [];
  const fetch = async (input: string | URL | Request, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    const path = new URL(request.url).pathname;
    const text = await request.clone().text();
    sent.push({
      path,
      body: text ? (JSON.parse(text) as Record<string, unknown>) : {},
      auth: request.headers.get("authorization"),
    });
    const answer = answers[path];
    if (typeof answer === "function") return (answer as () => Response)();
    return new Response(JSON.stringify({ value: answer ?? {} }), {
      headers: { "content-type": "application/json" },
    });
  };
  const client = createObopClient({
    baseUrl: "http://10.0.2.2:8080",
    getToken: () => "tok",
    fetch: fetch as typeof globalThis.fetch,
  });
  return { api: createTripApi(client, { timeoutMs: 5000 }), sent };
}

const TRIP = {
  id: "t-1",
  client_trip_id: "c1",
  state: "ended",
  purpose: "business",
  started_at: "2026-10-15T09:00:00.000000Z",
  ended_at: "2026-10-15T09:10:00.000000Z",
  distance_m: "1234.56",
  integrity: "",
  reject_reason: "",
  point_count: 120,
  max_seq: 120,
};

describe("the calls of the outbox", () => {
  test("a trip is created with bulk_create, one row and the key trip:<id>", async () => {
    const { api, sent } = instance({
      "/api/mileage_trip/bulk_create": { ok: true, ids: ["t-1"], replayed: false },
    });
    const id = await api.createTrip(
      { clientTripId: "c1", purpose: "commute", startedAt: "2026-10-15T09:00:00.000Z" },
      "trip:c1",
    );
    expect(id).toBe("t-1");
    expect(sent[0]).toEqual({
      path: "/api/mileage_trip/bulk_create",
      body: {
        rows: [
          { client_trip_id: "c1", purpose: "commute", started_at: "2026-10-15T09:00:00.000Z" },
        ],
        mode: "all_or_nothing",
        idempotency_key: "trip:c1",
      },
      auth: "Bearer tok",
    });
  });

  test("a report without the id of the trip is an invalid response, not a guess", async () => {
    const { api } = instance({ "/api/mileage_trip/bulk_create": { ok: true, ids: [null] } });
    await expect(
      api.createTrip({ clientTripId: "c1", purpose: "business", startedAt: "x" }, "trip:c1"),
    ).rejects.toMatchObject({ code: "invalid_response" });
  });

  test("points go to mileage_point.bulk_create under the key of their range", async () => {
    const { api, sent } = instance({ "/api/mileage_point/bulk_create": { ok: true } });
    const row = {
      trip: "t-1",
      seq: 1,
      recorded_at: "2026-10-15T09:00:00.000Z",
      lat: "52.0000000",
      lon: "13.0000000",
    };
    await api.createPoints([row], "points:c1:1-1");
    expect(sent[0]!.body).toEqual({
      rows: [row],
      mode: "all_or_nothing",
      idempotency_key: "points:c1:1-1",
    });
  });

  test("the trip is read from get, with empty texts as absent", async () => {
    const { api, sent } = instance({ "/api/mileage_trip/get": TRIP });
    const trip = await api.getTrip("t-1");
    expect(sent[0]!.body).toEqual({ id: "t-1" });
    expect(trip).toEqual({
      id: "t-1",
      clientTripId: "c1",
      state: "ended",
      purpose: "business",
      startedAt: "2026-10-15T09:00:00.000000Z",
      endedAt: "2026-10-15T09:10:00.000000Z",
      distanceM: "1234.56",
      integrity: null,
      rejectReason: null,
      pointCount: 120,
      maxSeq: 120,
    });
  });

  test("a trip is found by its client id with a search", async () => {
    const { api, sent } = instance({
      "/api/mileage_trip/search": { rows: [TRIP], total: 1 },
    });
    expect((await api.findTrip("c1"))?.id).toBe("t-1");
    expect(sent[0]!.body).toEqual({
      filter: [{ field: "client_trip_id", op: "eq", value: "c1" }],
      limit: 1,
    });
    const none = instance({ "/api/mileage_trip/search": { rows: [], total: 0 } });
    expect(await none.api.findTrip("c1")).toBeNull();
  });

  test("the list is asked newest first", async () => {
    const { api, sent } = instance({ "/api/mileage_trip/search": { rows: [TRIP], total: 1 } });
    expect(await api.listTrips(50)).toHaveLength(1);
    expect(sent[0]!.body).toEqual({ order: [{ field: "started_at", dir: "desc" }], limit: 50 });
  });

  test("update, end, submit and discard send the id and the snake_case fields", async () => {
    const { api, sent } = instance({
      "/api/mileage_trip/update": TRIP,
      "/api/mileage_trip/end": TRIP,
      "/api/mileage_trip/submit": { ...TRIP, state: "in_review" },
      "/api/mileage_trip/discard": { ...TRIP, state: "discarded" },
    });
    await api.updateTrip("t-1", {
      endedAt: "2026-10-15T09:10:00.000Z",
      expectedPoints: 120,
      lastSeq: 120,
    });
    await api.updateTrip("t-1", { discardReason: "Private" });
    await api.endTrip("t-1");
    expect((await api.submitTrip("t-1")).state).toBe("in_review");
    expect((await api.discardTrip("t-1")).state).toBe("discarded");
    expect(sent.map((s) => [s.path.split("/").pop(), s.body])).toEqual([
      [
        "update",
        { id: "t-1", ended_at: "2026-10-15T09:10:00.000Z", expected_points: 120, last_seq: 120 },
      ],
      ["update", { id: "t-1", discard_reason: "Private" }],
      ["end", { id: "t-1" }],
      ["submit", { id: "t-1" }],
      ["discard", { id: "t-1" }],
    ]);
  });

  test("an error answer reaches the caller as ObopApiError with the code and the wait", async () => {
    const { api } = instance({
      "/api/mileage_trip/end": () =>
        new Response(JSON.stringify({ error: "2 of 5 points are stored", code: "incomplete" }), {
          status: 409,
          headers: { "content-type": "application/json" },
        }),
    });
    const error = await api.endTrip("t-1").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ObopApiError);
    expect(error).toMatchObject({ status: 409, code: "incomplete" });
  });
});

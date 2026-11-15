import { describe, expect, test } from "vite-plus/test";
import { checkAddress } from "./address.ts";
import { bindOwner, bindingOf, parseOwner, takeOver, unsentOf } from "./identity.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import type { NewFix } from "./types.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const T0 = Date.parse("2026-10-15T09:00:00Z");
const A = { origin: "http://h:1", user: "dave" };
const B = { origin: "http://h:1", user: "rita" };
const ELSEWHERE = { origin: "https://other.example", user: "dave" };

const fixes = (n: number): NewFix[] =>
  Array.from({ length: n }, (_, i) => ({
    t: T0 + i * 5000,
    lat: 52 + i * 0.00006,
    lon: 13,
    accuracyM: 5,
    speedMps: 1.4,
  }));

async function rig() {
  const db = memoryDb();
  await migrate(db);
  return createSqlStore(db);
}

/** A trip of the previous owner that was stopped and has points the server never took. */
async function unsentTrip(store: Awaited<ReturnType<typeof rig>>, id = "c1", n = 4) {
  await store.startTrip({
    clientTripId: id,
    purpose: "business",
    startedAt: new Date(T0).toISOString(),
    nowMs: T0,
  });
  await store.appendFixes(id, fixes(n));
  await store.stopTrip(id, new Date(T0 + n * 5000).toISOString());
}

describe("whose trips the outbox holds", () => {
  test("the first sign-in becomes the owner", async () => {
    const store = await rig();
    expect(await bindOwner(store, A)).toEqual({ kind: "own" });
    expect(parseOwner(await store.getMeta("owner"))).toEqual(A);
  });

  test("the same person at the same address gets the trips back", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    expect(await bindOwner(store, A)).toEqual({ kind: "own" });
    expect((await store.listTrips()).map((t) => t.clientTripId)).toEqual(["c1"]);
  });

  test("another person finds the unsent trips foreign, and nothing is deleted", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store, "c1", 4);
    await unsentTrip(store, "c2", 3);
    expect(await bindOwner(store, B)).toEqual({ kind: "foreign", unsent: { trips: 2, points: 7 } });
    expect(parseOwner(await store.getMeta("owner"))).toEqual(A); // still the previous owner's
    expect(await store.pointCount("c1")).toBe(4);
    expect(await store.pointCount("c2")).toBe(3);
  });

  test("the same person at another address is foreign too", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    expect(await bindOwner(store, ELSEWHERE)).toMatchObject({ kind: "foreign" });
  });

  test("a stopped trip with a refusal counts as unsent: its points are only here", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    await store.patchTrip("c1", { problem: { step: "points", code: "bad_request", message: "x" } });
    expect(await unsentOf(store)).toEqual({ trips: 1, points: 4 });
  });

  test("another person finds an outbox with nothing unsent: it is emptied and taken over", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    await store.patchTrip("c1", { endDone: true, purgePoints: true }); // the server has it all
    expect(await bindOwner(store, B)).toEqual({ kind: "own" });
    expect(await store.listTrips()).toEqual([]); // the other person's list of trips is gone
    expect(parseOwner(await store.getMeta("owner"))).toEqual(B);
  });

  test("the choice to delete and go on empties the outbox for the new owner", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    await takeOver(store, B);
    expect(await store.listTrips()).toEqual([]);
    expect(await bindOwner(store, B)).toEqual({ kind: "own" });
  });

  test("at start, the stored session and the outbox are compared again", async () => {
    const store = await rig();
    await bindOwner(store, A);
    await unsentTrip(store);
    const config = { baseUrl: "http://h:1", clientId: "c" };
    expect(await bindingOf(store, config)).toEqual({ kind: "own" }); // identity not known yet
    expect(await bindingOf(store, { ...config, user: "dave" })).toEqual({ kind: "own" });
    expect(await bindingOf(store, { ...config, user: "rita" })).toEqual({
      kind: "foreign",
      unsent: { trips: 1, points: 4 },
    });
  });

  test("a damaged owner record is read as no owner", () => {
    expect(parseOwner("{not json")).toBeNull();
    expect(parseOwner('{"origin":1}')).toBeNull();
    expect(parseOwner(null)).toBeNull();
  });
});

describe("the address of the instance", () => {
  test.each([
    ["https://obop.example.com", "https://obop.example.com"],
    ["  https://Obop.Example.com/  ", "https://obop.example.com"],
    ["https://obop.example.com:443", "https://obop.example.com"],
    ["https://obop.example.com:8443", "https://obop.example.com:8443"],
  ])("%s is %s", (input, address) => {
    expect(checkAddress(input, false)).toEqual({ ok: true, address });
  });

  test("http is only for a build made for a test instance", () => {
    expect(checkAddress("http://10.0.2.2:18080", false)).toEqual({
      ok: false,
      problem: "cleartext",
    });
    expect(checkAddress("http://10.0.2.2:18080", true)).toEqual({
      ok: true,
      address: "http://10.0.2.2:18080",
    });
  });

  test.each([
    "",
    "obop.example.com",
    "ftp://obop.example.com",
    "https://user:secret@obop.example.com",
    "https://obop.example.com/app",
    "https://obop.example.com/?a=1",
    "https://obop.example.com/#x",
    "javascript:alert(1)",
  ])("%j is not an address", (input) => {
    expect(checkAddress(input, true)).toEqual({ ok: false, problem: "address" });
  });
});

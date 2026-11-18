// The app's calls against a real instance (L35): `obop serve --dev-auth` on a free port and a
// throwaway SQLite database, started by this file and stopped when it ends. The fakes of the other
// tests answer as the server is believed to; this one asks the server itself, so a wire name that
// both sides got wrong fails here. It runs only when OBOP_BIN names the binary, so `vp test` on
// its own skips it:
//
//   OBOP_BIN=/path/to/obop vp test instance
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { ObopApiError, createObopClient } from "@obop/client";
import { afterAll, beforeAll, describe, expect, test, vi } from "vite-plus/test";
import { createTripApi, type TripApi } from "./api.ts";
import { createEngine } from "./engine.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import type { NewFix } from "./types.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const binary = process.env.OBOP_BIN ? resolve(process.env.OBOP_BIN) : undefined;

function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as { port: number };
      server.close(() => ok(port));
    });
  });
}

/** A walk north at about 1.4 m/s: `n` fixes 5 s apart, starting at `t0`. */
function walk(t0: number, n: number): NewFix[] {
  return Array.from({ length: n }, (_, i) => ({
    t: t0 + i * 5000,
    lat: 52 + i * 0.00006,
    lon: 13,
    accuracyM: 5,
    speedMps: 1.4,
  }));
}

/** A random UUID version 4, as the app makes one at Start. */
const uuid = () => crypto.randomUUID();

describe.skipIf(!binary)("the Mileage app against a running instance", () => {
  let child: ChildProcess | undefined;
  let exited: Promise<void> = Promise.resolve();
  let dir = "";
  let baseUrl = "";
  let log = "";

  const api = (token = "dev-admin"): TripApi =>
    createTripApi(createObopClient({ baseUrl, getToken: () => token }));

  beforeAll(async () => {
    if (!binary || !existsSync(binary)) throw new Error(`OBOP_BIN ${binary} does not exist`);
    dir = mkdtempSync(join(tmpdir(), "obop-mileage-"));
    const port = await freePort();
    baseUrl = `http://127.0.0.1:${port}`;
    // The default limit of 12 bulk calls a minute per person stands: one test meets it.
    const env = { ...process.env };
    delete env.OBOP_BULK_PER_MINUTE;
    const server = spawn(
      binary,
      [
        "serve",
        "--dev-auth",
        "--addr",
        `127.0.0.1:${port}`,
        "--db",
        `sqlite://${join(dir, "mileage.sqlite")}`,
      ],
      { stdio: ["ignore", "ignore", "pipe"], env },
    );
    child = server;
    server.stderr?.on("data", (chunk: Buffer) => {
      log += chunk.toString();
    });
    exited = new Promise((done) => server.once("exit", () => done()));
    await vi.waitFor(
      async () => {
        if (server.exitCode !== null) throw new Error(`obop exited early:\n${log}`);
        const res = await fetch(`${baseUrl}/healthz`);
        if (!res.ok) throw new Error(`healthz answered ${res.status}`);
      },
      { timeout: 45_000, interval: 100 },
    );
  }, 60_000);

  afterAll(async () => {
    child?.kill();
    await exited;
    if (dir) rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });

  test("the engine records, uploads, ends and submits a trip, and a replay changes nothing", async () => {
    const trips = api();
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const engine = createEngine({ store, api: trips });
    const id = uuid();
    const t0 = Date.now() - 60_000;
    await store.startTrip({
      clientTripId: id,
      purpose: "business",
      startedAt: new Date(t0).toISOString(),
      nowMs: t0,
    });
    await store.appendFixes(id, walk(t0, 10));
    await store.stopTrip(id, new Date(t0 + 50_000).toISOString());
    await store.requestSubmit(id);
    const report = await engine.drain();
    expect(report, JSON.stringify(report)).toMatchObject({ pending: 0, problems: 0 });
    const local = await store.getTrip(id);
    expect(local?.serverId).toEqual(expect.any(String));
    // The server's view: in review, the distance it computed from the ten points.
    const server = await trips.getTrip(local!.serverId!);
    expect(server).toMatchObject({
      clientTripId: id,
      state: "in_review",
      pointCount: 10,
      maxSeq: 10,
    });
    expect(Number(server.distanceM)).toBeGreaterThan(55);
    expect((await trips.findTrip(id))?.id).toBe(local!.serverId);
    expect((await trips.listTrips(50)).map((t) => t.id)).toContain(local!.serverId);
    // A repeat of the create with the same key is the first answer, not a second trip.
    const again = await trips.createTrip(
      { clientTripId: id, purpose: "business", startedAt: new Date(t0).toISOString() },
      `trip:${id}`,
    );
    expect(again).toBe(local!.serverId);
    // Nothing is left to send.
    expect(await engine.drain()).toMatchObject({ state: "idle", problems: 0 });
  });

  test("the actions answer with the trip, and a batch repeated with its key is the first answer", async () => {
    const trips = api();
    const id = uuid();
    const t0 = Date.now() - 60_000;
    const serverId = await trips.createTrip(
      { clientTripId: id, purpose: "commute", startedAt: new Date(t0).toISOString() },
      `trip:${id}`,
    );
    const rows = walk(t0, 4).map((f, i) => ({
      trip: serverId,
      seq: i + 1,
      recorded_at: new Date(f.t).toISOString(),
      lat: f.lat.toFixed(7),
      lon: f.lon.toFixed(7),
      accuracy_m: "5",
    }));
    await trips.createPoints(rows, `points:${id}:1-4`);
    await trips.createPoints(rows, `points:${id}:1-4`);
    await trips.updateTrip(serverId, { expectedPoints: 4, lastSeq: 4 });
    const ended = await trips.endTrip(serverId);
    expect(ended).toMatchObject({ id: serverId, clientTripId: id, state: "ended", pointCount: 4 });
    const submitted = await trips.submitTrip(serverId);
    expect(submitted).toMatchObject({ id: serverId, state: "in_review" });
  });

  test("a new bulk call over 12 in a minute is refused with rate_limited, a replay is not", async () => {
    // The tests above used some of this person's 12 calls of the minute; new keys use up the rest.
    const trips = api();
    const t0 = Date.now() - 60_000;
    const row = (clientTripId: string) => ({
      clientTripId,
      purpose: "business" as const,
      startedAt: new Date(t0).toISOString(),
    });
    const first = uuid();
    const firstId = await trips.createTrip(row(first), `trip:${first}`);
    let refused: unknown = null;
    let made = 1;
    while (refused === null && made <= 12) {
      const next = uuid();
      try {
        await trips.createTrip(row(next), `trip:${next}`);
        made++;
      } catch (error) {
        refused = error;
      }
    }
    expect(refused).toBeInstanceOf(ObopApiError);
    expect(refused).toMatchObject({ status: 429, code: "rate_limited" });
    // The answered call, repeated, is the first answer and is never refused for the limit.
    expect(await trips.createTrip(row(first), `trip:${first}`)).toBe(firstId);
  });
});

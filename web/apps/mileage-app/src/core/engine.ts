import { ObopApiError, ObopNetworkError, retryDelayMs } from "@obop/client";
import type { TripApi } from "./api.ts";
import { hasWork, type OutboxStore, type TripPatch } from "./store.ts";
import { MAX_BATCH_POINTS, type LocalTrip, type Problem, type RemoteTrip } from "./types.ts";
import { pointRow, pointsKey, tripKey } from "./values.ts";

/** How a call failed, reduced to what the outbox does about it. */
export type Failure =
  /** No connection, a timeout, or a call that broke: repeat later, with the same key. */
  | { kind: "offline" }
  /** 429: refused before it ran; repeat after the wait. */
  | { kind: "rate_limited"; waitMs: number }
  /** 5xx: the server could not do it now; repeat later. */
  | { kind: "unavailable"; waitMs: number | null }
  /** 401: the token is gone. Nothing is lost; the driver signs in again. */
  | { kind: "signed_out" }
  /** 409 `incomplete`: points are still missing on the server; repeat `end` with backoff. */
  | { kind: "incomplete"; waitMs: number | null; message: string }
  /** Any other 4xx: repeating changes nothing. */
  | { kind: "refused"; status: number; code: string; message: string };

export function classify(error: unknown): Failure {
  if (!(error instanceof ObopApiError) || error instanceof ObopNetworkError) {
    return { kind: "offline" };
  }
  if (error.status === 401) return { kind: "signed_out" };
  if (error.status === 429) return { kind: "rate_limited", waitMs: retryDelayMs(error, 60_000) };
  if (error.status === 409 && error.code === "incomplete") {
    return {
      kind: "incomplete",
      waitMs: error.retryAfterSeconds === undefined ? null : retryDelayMs(error),
      message: error.message,
    };
  }
  if (error.status >= 500 && error.applied !== "partial") {
    return {
      kind: "unavailable",
      waitMs: error.retryAfterSeconds === undefined ? null : retryDelayMs(error),
    };
  }
  return {
    kind: "refused",
    status: error.status,
    code: error.code ?? "bad_request",
    message: error.hint ? `${error.message} ${error.hint}` : error.message,
  };
}

/** Stops the whole run (the failure is not about one trip). */
class Halt extends Error {
  readonly failure: Exclude<Failure, { kind: "refused" } | { kind: "incomplete" }>;
  constructor(failure: Halt["failure"]) {
    super(failure.kind);
    this.failure = failure;
  }
}

/** The server says points are still missing: repeat `end` later. Ends the work on this trip for now. */
class Incomplete extends Error {
  readonly waitMs: number | null;
  constructor(message: string, waitMs: number | null) {
    super(message);
    this.waitMs = waitMs;
  }
}

/** The server refused for good. `step` is the step of the trip that was refused. */
class Refused extends Error {
  readonly failure: Extract<Failure, { kind: "refused" }>;
  readonly step: Problem["step"];
  constructor(step: Problem["step"], failure: Refused["failure"]) {
    super(failure.message);
    this.failure = failure;
    this.step = step;
  }
}

export type DrainState = "idle" | "ok" | "offline" | "rate_limited" | "unavailable" | "signed_out";

export interface DrainReport {
  state: DrainState;
  /** Trips that still have something to send (not counting those stopped by a refusal). */
  pending: number;
  /** Trips stopped by a refusal for good: the driver has to act. */
  problems: number;
  /** When to try again, in milliseconds; `null` when nothing is waiting for a time. */
  retryInMs: number | null;
}

export interface Engine {
  /** One pass over everything the phone still has to tell the server. Never runs twice at once. */
  drain(): Promise<DrainReport>;
  /** Reads the trip from the server and updates what the phone shows. */
  refreshTrip(clientTripId: string): Promise<void>;
  /** Reads the driver's own trips from the server (those recorded elsewhere too). */
  refreshRemote(limit?: number): Promise<void>;
}

export interface EngineOptions {
  store: OutboxStore;
  api: TripApi;
  /** Milliseconds between retries when the server gave none: doubles from `baseMs` up to `maxMs`. */
  backoff?: { baseMs: number; maxMs: number };
}

/** After this many `incomplete` answers in a row the driver is told; the retries go on. */
const INCOMPLETE_NOTICE = 5;

const serverWords = (state: string): string => state.replace("_", " ");

export function createEngine({
  store,
  api,
  backoff = { baseMs: 2000, maxMs: 300_000 },
}: EngineOptions): Engine {
  let running: Promise<DrainReport> | null = null;
  let again = false;
  let failures = 0;

  const wait = (): number =>
    Math.min(backoff.baseMs * 2 ** Math.max(0, failures - 1), backoff.maxMs);

  /** Runs one call of `step`. A failure that is not about this trip becomes a `Halt`. */
  async function call<T>(step: Problem["step"], fn: () => Promise<T>): Promise<T> {
    try {
      return await fn();
    } catch (error) {
      const failure = classify(error);
      if (failure.kind === "refused") throw new Refused(step, failure);
      if (failure.kind === "incomplete") throw new Incomplete(failure.message, failure.waitMs);
      throw new Halt(failure);
    }
  }

  const fromServer = (r: RemoteTrip): TripPatch => ({
    serverState: r.state,
    distanceM: r.distanceM,
    integrity: r.integrity,
    rejectReason: r.rejectReason,
  });

  const conflict = (step: Problem["step"], what: string, state: string): Refused =>
    new Refused(step, {
      kind: "refused",
      status: 400,
      code: "state_conflict",
      message: `A trip that is ${serverWords(state)} cannot be ${what}.`,
    });

  /** The server id of the trip: makes the trip on the server if it is not there yet. */
  async function serverId(t: LocalTrip): Promise<string> {
    if (t.serverId !== null) return t.serverId;
    await store.patchTrip(t.clientTripId, { createAttempted: true });
    let id: string;
    try {
      id = await call("create", () =>
        api.createTrip(
          { clientTripId: t.clientTripId, purpose: t.purpose, startedAt: t.startedAt },
          tripKey(t.clientTripId),
        ),
      );
    } catch (error) {
      if (!(error instanceof Refused)) throw error;
      // The key is forgotten after 24 hours and the unique constraint refuses the repeat: the trip
      // may be there already. Ask for it by the id the phone made.
      const found = await call("create", () => api.findTrip(t.clientTripId));
      if (!found) throw error;
      id = found.id;
    }
    await store.patchTrip(t.clientTripId, { serverId: id });
    return id;
  }

  /** Sends every batch that waits. A refusal of a batch is read against what the server holds. */
  async function uploadPoints(t: LocalTrip, id: string): Promise<void> {
    for (;;) {
      const batch = await store.nextBatch(t.clientTripId, MAX_BATCH_POINTS);
      if (!batch) return;
      const rows = (await store.batchPoints(batch)).map((p) => pointRow(id, p));
      try {
        await call("points", () =>
          api.createPoints(rows, pointsKey(t.clientTripId, batch.firstSeq, batch.lastSeq)),
        );
      } catch (error) {
        if (!(error instanceof Refused)) throw error;
        // Refused for good, or the 24 hour memory of the key is gone and the unique constraint of
        // (trip, seq) refused the repeat: if the server holds these points, they are stored.
        const server = await call("points", () => api.getTrip(id));
        const stored =
          server.pointCount !== null &&
          server.maxSeq !== null &&
          server.pointCount >= batch.lastSeq &&
          server.maxSeq >= batch.lastSeq;
        if (!stored) throw error;
      }
      await store.markBatchSent(batch);
      failures = 0;
    }
  }

  async function discard(t: LocalTrip): Promise<void> {
    if (t.serverId === null && !t.createAttempted) {
      // The server never heard of the trip: nothing to discard there.
      await store.purgePoints(t.clientTripId);
      await store.patchTrip(t.clientTripId, { discardDone: true, serverState: "discarded" });
      return;
    }
    const id = await serverId(t);
    const server = await call("discard", () => api.getTrip(id));
    if (server.state !== "discarded") {
      if (server.state !== "recording" && server.state !== "ended" && server.state !== "rejected") {
        throw conflict("discard", "discarded", server.state);
      }
      await call("discard", () => api.updateTrip(id, { discardReason: t.discardReason ?? "" }));
      await call("discard", () => api.discardTrip(id));
    }
    await store.purgePoints(t.clientTripId);
    await store.patchTrip(t.clientTripId, { discardDone: true, serverState: "discarded" });
  }

  async function end(t: LocalTrip, id: string): Promise<void> {
    const server = await call("end", () => api.getTrip(id));
    if (server.state !== "recording") {
      // A repeat of a call whose answer was lost: the trip is ended already.
      await store.patchTrip(t.clientTripId, {
        endDone: true,
        attempts: 0,
        problem: null,
        ...fromServer(server),
      });
      await store.purgePoints(t.clientTripId);
      return;
    }
    await call("end", () =>
      api.updateTrip(id, {
        endedAt: t.endedAt ?? undefined,
        expectedPoints: t.expectedPoints ?? 0,
        lastSeq: t.lastSeq ?? 0,
      }),
    );
    let ended: RemoteTrip;
    try {
      ended = await call("end", () => api.endTrip(id));
    } catch (error) {
      if (error instanceof Incomplete) {
        const attempts = t.attempts + 1;
        await store.patchTrip(t.clientTripId, {
          attempts,
          // From the fifth answer the driver is told; the outbox keeps trying.
          ...(attempts >= INCOMPLETE_NOTICE
            ? { problem: { step: "end", code: "incomplete", message: error.message } as Problem }
            : {}),
        });
      }
      throw error;
    }
    await store.patchTrip(t.clientTripId, {
      endDone: true,
      attempts: 0,
      problem: null,
      ...fromServer(ended),
    });
    // The server has the route and the distance; the phone does not keep the route.
    await store.purgePoints(t.clientTripId);
  }

  async function submit(t: LocalTrip, id: string): Promise<void> {
    const server = await call("submit", () => api.getTrip(id));
    if (server.state === "in_review" || server.state === "confirmed") {
      await store.patchTrip(t.clientTripId, { submitDone: true, ...fromServer(server) });
      return;
    }
    if (server.state !== "ended") throw conflict("submit", "submitted", server.state);
    const submitted = await call("submit", () => api.submitTrip(id));
    await store.patchTrip(t.clientTripId, { submitDone: true, ...fromServer(submitted) });
  }

  /** The work of one trip, in the order the server needs it. */
  async function syncTrip(t: LocalTrip): Promise<void> {
    if (t.discardReason !== null) return discard(t);
    if (t.origin === "server") {
      if (t.submitRequested && !t.submitDone && t.serverId !== null) await submit(t, t.serverId);
      return;
    }
    const id = await serverId(t);
    await uploadPoints(t, id);
    if (!t.stopRequested) return;
    if (!t.endDone) await end(t, id);
    if (t.submitRequested && !t.submitDone) await submit(t, id);
  }

  async function pass(): Promise<DrainReport> {
    let state: DrainState = "ok";
    let retryIn: number | null = null;
    const soonest = (ms: number | null) => {
      if (ms !== null) retryIn = retryIn === null ? ms : Math.min(retryIn, ms);
    };
    const trips = (await store.listTrips())
      .filter(hasWork)
      .sort((a, b) => a.createdMs - b.createdMs);
    if (trips.length === 0) return { state: "idle", pending: 0, problems: 0, retryInMs: null };
    for (const listed of trips) {
      try {
        // Read again: the driver may have pressed Stop or Discard since the list was made.
        const t = (await store.getTrip(listed.clientTripId)) ?? listed;
        if (hasWork(t)) await syncTrip(t);
      } catch (error) {
        if (error instanceof Incomplete) {
          soonest(error.waitMs ?? backoff.baseMs * 2 ** 2);
          continue;
        }
        if (error instanceof Refused) {
          await store.patchTrip(listed.clientTripId, {
            problem: { step: error.step, code: error.failure.code, message: error.failure.message },
          });
          continue;
        }
        if (error instanceof Halt) {
          failures += 1;
          const f = error.failure;
          state = f.kind;
          soonest(
            f.kind === "signed_out"
              ? null
              : f.kind === "rate_limited"
                ? f.waitMs
                : f.kind === "unavailable"
                  ? (f.waitMs ?? wait())
                  : wait(),
          );
          break;
        }
        throw error;
      }
    }
    const after = await store.listTrips();
    const pending = after.filter(hasWork).length;
    const problems = after.filter((t) => t.problem !== null && t.origin === "device").length;
    if (state === "ok") failures = 0;
    return {
      state: pending === 0 && state === "ok" ? "idle" : state,
      pending,
      problems,
      retryInMs: pending === 0 ? null : retryIn,
    };
  }

  return {
    async drain() {
      if (running) {
        again = true;
        return running;
      }
      const run = (async () => {
        let report = await pass();
        // A request that came while the pass ran gets one more pass.
        while (again && (report.state === "ok" || report.state === "idle")) {
          again = false;
          report = await pass();
        }
        return report;
      })();
      running = run;
      try {
        return await run;
      } finally {
        running = null;
        again = false;
      }
    },

    async refreshTrip(clientTripId) {
      const t = await store.getTrip(clientTripId);
      if (!t || t.serverId === null) return;
      await store.patchTrip(clientTripId, fromServer(await api.getTrip(t.serverId)));
    },

    async refreshRemote(limit = 50) {
      await store.mergeRemote(await api.listTrips(limit));
    },
  };
}

import { ObopApiError, ObopNetworkError } from "@obop/client";
import type { TripApi } from "../core/api.ts";
import type { Purpose, RemoteTrip, ServerState } from "../core/types.ts";
import type { PointRow } from "../core/values.ts";

export type Op =
  | "createTrip"
  | "findTrip"
  | "getTrip"
  | "listTrips"
  | "createPoints"
  | "updateTrip"
  | "endTrip"
  | "submitTrip"
  | "discardTrip";

interface Stored {
  id: string;
  clientTripId: string;
  purpose: Purpose;
  startedAt: string;
  endedAt: string | null;
  state: ServerState;
  expectedPoints: number | null;
  lastSeq: number | null;
  discardReason: string | null;
  distanceM: string | null;
}

export const apiError = (
  status: number,
  extra: {
    code?: string;
    message?: string;
    applied?: "none" | "unknown" | "partial";
    retryAfter?: number;
  } = {},
) =>
  new ObopApiError({
    status,
    message: extra.message ?? `refused ${status}`,
    ...(extra.code === undefined ? {} : { code: extra.code }),
    ...(extra.applied === undefined ? {} : { applied: extra.applied }),
    ...(extra.retryAfter === undefined ? {} : { retryAfterSeconds: extra.retryAfter }),
  });

export const networkError = () => new ObopNetworkError(new TypeError("Network request failed"));

interface Script {
  op: Op;
  /** Run the call on the server, then fail: the answer is lost. */
  after: boolean;
  error: unknown;
  times: number;
}

/**
 * A scripted obop server for the outbox: the rules of mileage (docs/spec/mobile.md 5.2 to 5.4) that
 * the app depends on, the memory of `bulk_create` keys, and failures that a test queues by hand.
 */
export class FakeServer implements TripApi {
  readonly trips = new Map<string, Stored>();
  readonly points = new Map<string, Map<number, PointRow>>();
  /** `bulk_create` keys: the first answer, as the server remembers it for 24 hours. */
  readonly keys = new Map<string, string>();
  readonly calls: { op: Op; key?: string; detail?: string }[] = [];
  private readonly scripts: Script[] = [];
  private counter = 0;
  /** The distance the server computes at `end`. */
  distance = "1234.56";

  /** The next `times` calls of `op` fail with `error` before they run. */
  fail(op: Op, error: unknown, times = 1) {
    this.scripts.push({ op, after: false, error, times });
  }

  /** The next call of `op` runs on the server and then fails: the phone never sees the answer. */
  loseAnswer(op: Op, error: unknown = networkError(), times = 1) {
    this.scripts.push({ op, after: true, error, times });
  }

  /** The server forgets its idempotency keys (24 hours pass). */
  forgetKeys() {
    this.keys.clear();
  }

  count(op: Op): number {
    return this.calls.filter((c) => c.op === op).length;
  }

  stored(clientTripId: string): Stored | undefined {
    return [...this.trips.values()].find((t) => t.clientTripId === clientTripId);
  }

  pointSeqs(id: string): number[] {
    return [...(this.points.get(id)?.keys() ?? [])].sort((a, b) => a - b);
  }

  private before(op: Op, key?: string, detail?: string) {
    this.calls.push({
      op,
      ...(key === undefined ? {} : { key }),
      ...(detail === undefined ? {} : { detail }),
    });
    const s = this.scripts.find((x) => x.op === op && !x.after && x.times > 0);
    if (s) {
      s.times--;
      throw s.error;
    }
  }

  private afterwards(op: Op) {
    const s = this.scripts.find((x) => x.op === op && x.after && x.times > 0);
    if (s) {
      s.times--;
      throw s.error;
    }
  }

  private remote(t: Stored): RemoteTrip {
    const seqs = this.pointSeqs(t.id);
    return {
      id: t.id,
      clientTripId: t.clientTripId,
      state: t.state,
      purpose: t.purpose,
      startedAt: t.startedAt,
      endedAt: t.endedAt,
      distanceM: t.distanceM,
      integrity: null,
      rejectReason: null,
      pointCount: seqs.length,
      maxSeq: seqs.length === 0 ? 0 : seqs[seqs.length - 1]!,
    };
  }

  private need(id: string): Stored {
    const t = this.trips.get(id);
    // The module answers a trip it does not hold (or may not show) with a plain 400 and no code.
    if (!t) throw apiError(400, { message: `no mileage_trip ${id}` });
    return t;
  }

  async createTrip(
    row: { clientTripId: string; purpose: Purpose; startedAt: string },
    key: string,
  ): Promise<string> {
    this.before("createTrip", key);
    let id = this.keys.get(key);
    if (id === undefined) {
      if (this.stored(row.clientTripId)) {
        throw apiError(400, {
          message:
            "1 of 1 row(s) are invalid, so nothing was created. row 1: client_trip_id is not unique",
        });
      }
      id = `trip-${++this.counter}`;
      this.trips.set(id, {
        id,
        clientTripId: row.clientTripId,
        purpose: row.purpose,
        startedAt: row.startedAt,
        endedAt: null,
        state: "recording",
        expectedPoints: null,
        lastSeq: null,
        discardReason: null,
        distanceM: null,
      });
      this.keys.set(key, id);
    }
    this.afterwards("createTrip");
    return id;
  }

  async findTrip(clientTripId: string): Promise<RemoteTrip | null> {
    this.before("findTrip");
    const t = this.stored(clientTripId);
    return t ? this.remote(t) : null;
  }

  async getTrip(id: string): Promise<RemoteTrip> {
    this.before("getTrip", undefined, id);
    return this.remote(this.need(id));
  }

  async listTrips(): Promise<RemoteTrip[]> {
    this.before("listTrips");
    return [...this.trips.values()].map((t) => this.remote(t));
  }

  async createPoints(rows: readonly PointRow[], key: string): Promise<void> {
    this.before("createPoints", key);
    if (!this.keys.has(key)) {
      const trip = this.need(rows[0]!.trip);
      if (trip.state !== "recording") throw apiError(400, { message: "the trip is not recording" });
      const map = this.points.get(trip.id) ?? new Map<number, PointRow>();
      if (rows.some((r) => map.has(r.seq))) {
        throw apiError(400, { message: "row: (trip, seq) is not unique, so nothing was created" });
      }
      for (const r of rows) map.set(r.seq, r);
      this.points.set(trip.id, map);
      this.keys.set(key, "stored");
    }
    this.afterwards("createPoints");
  }

  async updateTrip(
    id: string,
    fields: { endedAt?: string; expectedPoints?: number; lastSeq?: number; discardReason?: string },
  ): Promise<void> {
    this.before("updateTrip", undefined, id);
    const t = this.need(id);
    if (t.state !== "recording" && t.state !== "ended" && t.state !== "rejected") {
      throw apiError(400, { message: `a trip that is ${t.state} cannot be edited` });
    }
    if (fields.endedAt !== undefined) t.endedAt = fields.endedAt;
    if (fields.expectedPoints !== undefined) t.expectedPoints = fields.expectedPoints;
    if (fields.lastSeq !== undefined) t.lastSeq = fields.lastSeq;
    if (fields.discardReason !== undefined) t.discardReason = fields.discardReason;
    this.afterwards("updateTrip");
  }

  async endTrip(id: string): Promise<RemoteTrip> {
    this.before("endTrip", undefined, id);
    const t = this.need(id);
    if (t.state !== "recording") {
      throw apiError(400, { message: `cannot end a trip that is ${t.state}` });
    }
    const seqs = this.pointSeqs(id);
    const have = seqs.length;
    const highest = have === 0 ? 0 : seqs[seqs.length - 1]!;
    const expected = t.expectedPoints ?? 0;
    const last = t.lastSeq ?? 0;
    if (have < expected && highest <= last) {
      throw apiError(409, {
        code: "incomplete",
        message: `${have} of ${expected} points are stored`,
      });
    }
    if (have !== expected || (have > 0 && highest !== last)) {
      throw apiError(400, { message: `the trip holds ${have} points, the counts are wrong` });
    }
    if (have < 2) throw apiError(400, { message: "the trip has no distance: discard it instead" });
    t.state = "ended";
    t.distanceM = this.distance;
    this.afterwards("endTrip");
    return this.remote(t);
  }

  async submitTrip(id: string): Promise<RemoteTrip> {
    this.before("submitTrip", undefined, id);
    const t = this.need(id);
    if (t.state !== "ended") {
      throw apiError(400, { message: `cannot submit a trip that is ${t.state}` });
    }
    t.state = "in_review";
    this.afterwards("submitTrip");
    return this.remote(t);
  }

  async discardTrip(id: string): Promise<RemoteTrip> {
    this.before("discardTrip", undefined, id);
    const t = this.need(id);
    if (t.state !== "recording" && t.state !== "ended" && t.state !== "rejected") {
      throw apiError(400, { message: `cannot discard a trip that is ${t.state}` });
    }
    if (!t.discardReason) throw apiError(400, { message: "discard needs a reason" });
    t.state = "discarded";
    this.afterwards("discardTrip");
    return this.remote(t);
  }
}

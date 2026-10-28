import { ObopApiError, type ObopClient } from "@obop/client";
import { isServerState, PURPOSES, type Purpose, type RemoteTrip } from "./types.ts";
import type { PointRow } from "./values.ts";

/**
 * What the outbox asks of the server. The engine knows only this interface, so the tests give it a
 * scripted server; `createTripApi` is the one that talks to an instance.
 */
export interface TripApi {
  /** `mileage_trip.bulk_create` with one row and the key `trip:<client_trip_id>`; answers the server id. */
  createTrip(
    row: { clientTripId: string; purpose: Purpose; startedAt: string },
    key: string,
  ): Promise<string>;
  /** The trip with this `client_trip_id`, or `null`. Used when the 24 hour memory of a key is gone. */
  findTrip(clientTripId: string): Promise<RemoteTrip | null>;
  getTrip(id: string): Promise<RemoteTrip>;
  /** Own trips, newest first. */
  listTrips(limit: number): Promise<RemoteTrip[]>;
  /** `mileage_point.bulk_create`, all or nothing, with this key. */
  createPoints(rows: readonly PointRow[], key: string): Promise<void>;
  updateTrip(
    id: string,
    fields: { endedAt?: string; expectedPoints?: number; lastSeq?: number; discardReason?: string },
  ): Promise<void>;
  endTrip(id: string): Promise<RemoteTrip>;
  submitTrip(id: string): Promise<RemoteTrip>;
  discardTrip(id: string): Promise<RemoteTrip>;
}

interface TripValue {
  id?: string;
  client_trip_id?: string;
  state?: string;
  purpose?: string;
  started_at?: string;
  ended_at?: string;
  distance_m?: string;
  integrity?: string;
  reject_reason?: string;
  point_count?: number;
  max_seq?: number;
}

function toRemote(value: TripValue | undefined): RemoteTrip {
  if (!value || typeof value.id !== "string" || typeof value.client_trip_id !== "string") {
    throw new ObopApiError({
      status: 200,
      code: "invalid_response",
      message: "The server did not answer with a trip",
    });
  }
  const text = (v: string | undefined) => (typeof v === "string" && v !== "" ? v : null);
  return {
    id: value.id,
    clientTripId: value.client_trip_id,
    state: isServerState(value.state) ? value.state : "recording",
    purpose: (PURPOSES as readonly string[]).includes(value.purpose ?? "")
      ? (value.purpose as Purpose)
      : null,
    startedAt: text(value.started_at),
    endedAt: text(value.ended_at),
    distanceM: text(value.distance_m),
    integrity: text(value.integrity),
    rejectReason: text(value.reject_reason),
    pointCount: typeof value.point_count === "number" ? value.point_count : null,
    maxSeq: typeof value.max_seq === "number" ? value.max_seq : null,
  };
}

export interface TripApiOptions {
  /** A call that has no answer after this many milliseconds is given up (and may be repeated). */
  timeoutMs?: number;
}

/** The server's wording for a call: a typed client over an instance. */
export function createTripApi(client: ObopClient, options: TripApiOptions = {}): TripApi {
  const timeoutMs = options.timeoutMs ?? 120_000;

  async function run<T>(model: string, command: string, input: object): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const { value } = await client.call<T>(model, command, input, { signal: controller.signal });
      return value;
    } finally {
      clearTimeout(timer);
    }
  }

  async function bulk(model: string, rows: readonly object[], key: string) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      return await client.bulkCreate(model, rows as Record<string, unknown>[], {
        idempotencyKey: key,
        mode: "all_or_nothing",
        signal: controller.signal,
      });
    } finally {
      clearTimeout(timer);
    }
  }

  async function getTrip(id: string): Promise<RemoteTrip> {
    return toRemote(await run<TripValue>("mileage_trip", "get", { id }));
  }

  return {
    async createTrip(row, key) {
      const report = await bulk(
        "mileage_trip",
        [
          {
            client_trip_id: row.clientTripId,
            purpose: row.purpose,
            started_at: row.startedAt,
          },
        ],
        key,
      );
      const id = (report as { ids?: (string | null)[] }).ids?.[0];
      if (typeof id !== "string" || id === "") {
        throw new ObopApiError({
          status: 200,
          code: "invalid_response",
          message: "The server did not say which trip it created",
        });
      }
      return id;
    },

    async findTrip(clientTripId) {
      const result = await run<{ rows?: TripValue[] }>("mileage_trip", "search", {
        filter: [{ field: "client_trip_id", op: "eq", value: clientTripId }],
        limit: 1,
      });
      const row = result.rows?.[0];
      return row ? toRemote(row) : null;
    },

    getTrip,

    async listTrips(limit) {
      const result = await run<{ rows?: TripValue[] }>("mileage_trip", "search", {
        order: [{ field: "started_at", dir: "desc" }],
        limit,
      });
      return (result.rows ?? []).map(toRemote);
    },

    async createPoints(rows, key) {
      await bulk("mileage_point", rows, key);
    },

    async updateTrip(id, fields) {
      const input: Record<string, unknown> = { id };
      if (fields.endedAt !== undefined) input.ended_at = fields.endedAt;
      if (fields.expectedPoints !== undefined) input.expected_points = fields.expectedPoints;
      if (fields.lastSeq !== undefined) input.last_seq = fields.lastSeq;
      if (fields.discardReason !== undefined) input.discard_reason = fields.discardReason;
      await run("mileage_trip", "update", input);
    },

    // An action answers with the fields it changed and the id (handlers.rs of the mileage module),
    // not with the whole trip, so the trip is read after it. If the read fails, the action is done
    // and the caller repeats the step: the engine reads the trip first and sees what is left.
    async endTrip(id) {
      await run("mileage_trip", "end", { id });
      return getTrip(id);
    },

    async submitTrip(id) {
      await run("mileage_trip", "submit", { id });
      return getTrip(id);
    },

    async discardTrip(id) {
      await run("mileage_trip", "discard", { id });
      return getTrip(id);
    },
  };
}

/**
 * What the phone keeps about a trip. Pure data: nothing in `core/` imports React Native, Expo or a
 * Node API, so the whole outbox runs in `vp test` and on the phone alike.
 */

export type Purpose = "business" | "private" | "commute";
export const PURPOSES: readonly Purpose[] = ["business", "private", "commute"];

/** The `state` of a `mileage_trip` on the server (docs/spec/mobile.md 5.2). */
export type ServerState =
  | "recording"
  | "ended"
  | "in_review"
  | "confirmed"
  | "rejected"
  | "discarded";

export const SERVER_STATES: readonly ServerState[] = [
  "recording",
  "ended",
  "in_review",
  "confirmed",
  "rejected",
  "discarded",
];

export function isServerState(value: unknown): value is ServerState {
  return typeof value === "string" && (SERVER_STATES as readonly string[]).includes(value);
}

/** A refusal for good: the server will not do what the phone asked, and asking again changes nothing. */
export interface Problem {
  /** The step that was refused. */
  step: "create" | "points" | "end" | "submit" | "discard";
  /** The code of the server (`incomplete`, `state_conflict`, ...) or `bad_request`. */
  code: string;
  /** The server's own words, shown to the driver. */
  message: string;
}

export interface LocalTrip {
  clientTripId: string;
  /** The id the server gave the trip; `null` until the first create has been answered. */
  serverId: string | null;
  /** `device`: recorded here. `server`: read from the server (recorded on another phone or before a reinstall). */
  origin: "device" | "server";
  purpose: Purpose;
  /** RFC 3339 instants (canonical UTC, with milliseconds when the phone had them). */
  startedAt: string;
  endedAt: string | null;
  /** A create call has been sent at least once, so the server may hold the trip even if no answer came. */
  createAttempted: boolean;
  /** The driver pressed Stop: no further points are taken. */
  stopRequested: boolean;
  /** The counts fixed at Stop, sent before `end`. */
  expectedPoints: number | null;
  lastSeq: number | null;
  endDone: boolean;
  submitRequested: boolean;
  submitDone: boolean;
  /** Set when the driver discards the trip (a reason is required). */
  discardReason: string | null;
  discardDone: boolean;
  /** The last view of the server. */
  serverState: ServerState | null;
  distanceM: string | null;
  integrity: string | null;
  rejectReason: string | null;
  /** Calls that did not get through in a row (reset by a success); drives the backoff. */
  attempts: number;
  /** The refusal for good that stops the outbox for this trip, until the driver acts on it. */
  problem: Problem | null;
  createdMs: number;
}

/** The fields a server row can refresh without touching what the phone still has to send. */
export interface RemoteTrip {
  id: string;
  clientTripId: string;
  state: ServerState;
  purpose: Purpose | null;
  startedAt: string | null;
  endedAt: string | null;
  distanceM: string | null;
  integrity: string | null;
  rejectReason: string | null;
  pointCount: number | null;
  maxSeq: number | null;
}

/** A position as the phone's location service gives it. `t` is milliseconds since the Unix epoch. */
export interface NewFix {
  t: number;
  lat: number;
  lon: number;
  /** Radius in metres; `null` when the platform did not say. */
  accuracyM: number | null;
  /** Metres per second; `null` when the platform did not say (or said a negative number). */
  speedMps: number | null;
}

/** A stored position. The texts are exactly what is sent. */
export interface StoredPoint {
  seq: number;
  t: number;
  lat: string;
  lon: string;
  accuracyM: string | null;
  speedMps: string | null;
}

/**
 * A run of points that has been fixed for upload. Its range never changes after it is made, so a
 * repeat of the call is the same request (the idempotency key names the range).
 */
export interface Batch {
  clientTripId: string;
  firstSeq: number;
  lastSeq: number;
}

/** At most this many points are in one `bulk_create` call (D70). */
export const MAX_BATCH_POINTS = 1000;

/** A trip holds at most this many points on the server (docs/spec/mobile.md 5.3). */
export const MAX_POINTS_PER_TRIP = 20000;

import type {
  Batch,
  LocalTrip,
  NewFix,
  Problem,
  Purpose,
  RemoteTrip,
  ServerState,
  StoredPoint,
} from "./types.ts";

/** A second trip cannot start while one is recording. */
export class TripAlreadyActive extends Error {
  readonly clientTripId: string;
  constructor(clientTripId: string) {
    super("A trip is already recording");
    this.name = "TripAlreadyActive";
    this.clientTripId = clientTripId;
  }
}

/** What changes on a trip row when the server has answered. Absent keys stay as they are. */
export interface TripPatch {
  serverId?: string | null;
  endedAt?: string | null;
  createAttempted?: boolean;
  endDone?: boolean;
  submitRequested?: boolean;
  submitDone?: boolean;
  discardDone?: boolean;
  serverState?: ServerState | null;
  distanceM?: string | null;
  integrity?: string | null;
  rejectReason?: string | null;
  attempts?: number;
  problem?: Problem | null;
  /** Remove the stored points and batches of the trip in the same transaction as the other changes. */
  purgePoints?: boolean;
}

/**
 * The phone's outbox: the trips, their points and the batches that are fixed for upload. It is the
 * only place a recorded point lives until the server has stored it. Every method that changes more
 * than one row is one transaction in the SQLite store.
 */
export interface OutboxStore {
  /** Starts a trip: stores it as the active one. Throws `TripAlreadyActive` while another is recording. */
  startTrip(input: {
    clientTripId: string;
    purpose: Purpose;
    startedAt: string;
    nowMs: number;
  }): Promise<LocalTrip>;
  /** The active (recording) trip, or `null`. */
  activeTrip(): Promise<LocalTrip | null>;
  getTrip(clientTripId: string): Promise<LocalTrip | null>;
  /** Newest first. */
  listTrips(): Promise<LocalTrip[]>;
  /**
   * Adds fixes to a trip that is recording, in order, with consecutive `seq` from 1. A fix not
   * newer than the last stored one is dropped (the server compares the times). Answers the number
   * stored, and 0 for a trip that is not recording (no longer active, stopped or discarded).
   */
  appendFixes(clientTripId: string, fixes: readonly NewFix[]): Promise<number>;
  /**
   * Stop: no more points, the counts are fixed (`expected_points` and `last_seq` are what the phone
   * holds) and the trip is no longer the active one. Repeating it changes nothing.
   */
  stopTrip(clientTripId: string, endedAt: string): Promise<LocalTrip>;
  requestSubmit(clientTripId: string): Promise<void>;
  /** The driver discards the trip. Stops the recording too if it was still running. */
  requestDiscard(clientTripId: string, reason: string, endedAt: string): Promise<void>;
  /** The driver asks to try again after a refusal: clears the problem and the attempts. */
  clearProblem(clientTripId: string): Promise<void>;
  patchTrip(clientTripId: string, patch: TripPatch): Promise<void>;
  /** Trips the server listed. A trip the phone knows keeps what it still has to send. */
  mergeRemote(trips: readonly RemoteTrip[]): Promise<void>;

  /** All stored points of a trip, in order of `seq`. */
  points(clientTripId: string): Promise<StoredPoint[]>;
  pointCount(clientTripId: string): Promise<number>;
  /**
   * The next run of points to send: the oldest batch not yet sent, or else a new one made of up to
   * `max` stored points that no batch holds. `null` when there is nothing to send.
   */
  nextBatch(clientTripId: string, max: number): Promise<Batch | null>;
  batchPoints(batch: Batch): Promise<StoredPoint[]>;
  markBatchSent(batch: Batch): Promise<void>;
  /** The route is not kept once the server has it, or the trip is discarded. */
  purgePoints(clientTripId: string): Promise<void>;
  /** Removes a trip that was never sent anywhere (only the local data). */
  deleteTrip(clientTripId: string): Promise<void>;
  /** Everything the phone holds about trips: for a sign-out that the driver confirmed. */
  clearAll(): Promise<void>;

  getMeta(key: string): Promise<string | null>;
  setMeta(key: string, value: string | null): Promise<void>;
}

/** Whether the outbox still has something to tell the server about this trip. */
export function hasWork(t: LocalTrip): boolean {
  // A refusal for good stops the trip until the driver acts; "incomplete" is only a notice, the
  // outbox keeps asking.
  if (t.problem !== null && t.problem.code !== "incomplete") return false;
  if (t.discardReason !== null) return !t.discardDone;
  if (t.origin === "server") return t.submitRequested && !t.submitDone;
  if (!t.stopRequested) return true; // points are uploaded while recording
  return !t.endDone || (t.submitRequested && !t.submitDone);
}

/**
 * Whether something about this trip exists only on this phone: points not yet taken over by the
 * server, a step still to send, or a refusal that stopped it. This is what a sign-out would delete,
 * so it is deliberately wider than `hasWork` (a trip stopped by a refusal still holds its route).
 */
export function holdsUnsent(t: LocalTrip, storedPoints: number): boolean {
  if (t.origin !== "device") return false;
  return storedPoints > 0 || hasWork(t) || t.problem !== null;
}

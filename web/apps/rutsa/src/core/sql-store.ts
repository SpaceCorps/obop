import { cleanDetails, cleanText } from "./details.ts";
import { analyseRecorded, progressOf, type SegmentTimes } from "./progress.ts";
import {
  TripAlreadyActive,
  TripNotFound,
  TripStateError,
  type ManualTrip,
  type TripRepository,
} from "./repository.ts";
import type { SqlDb, SqlTx } from "./sql.ts";
import { RECORDING_THINNING, acceptFixes, type Thinning } from "./thin.ts";
import {
  TRIP_SOURCES,
  TRIP_STATES,
  type NewFix,
  type StoredPoint,
  type Trip,
  type TripSource,
  type TripState,
} from "./types.ts";

/**
 * The trips in SQLite on the phone: `TripRepository` over the tables of `schema.ts`. The database
 * it is given is already migrated (`migrate` runs when the phone opens it, `native/database.ts`).
 * Every change runs in one transaction of `sql.ts`, so a failure leaves nothing half written, and
 * the location task and the screens cannot interleave inside one.
 *
 * After `createSqlStore` of `mileage-app` (`src/core/sql-store.ts`), without its upload state; the
 * distance is computed here, where `mileage-app` leaves it to its server.
 */
export interface SqlStoreOptions {
  /** A new trip id: a random UUID on the phone (`expo-crypto`), a counter in the tests. */
  newId: () => string;
  /** The thinning of stored fixes; `RECORDING_THINNING` unless a test gives another. */
  thinning?: Thinning;
}

interface TripRow {
  id: string;
  state: string;
  source: string;
  started_at: number;
  ended_at: number | null;
  distance_m: number | null;
  segment: number;
  /** `null` for a trip added by hand, which has no segments. */
  segment_started_at: number | null;
  name: string | null;
  from_text: string | null;
  to_text: string | null;
  note: string | null;
}

interface PointRow {
  trip_id: string;
  seq: number;
  segment: number;
  t: number;
  lat: number;
  lon: number;
  accuracy_m: number | null;
  speed_mps: number | null;
}

const TRIP_COLUMNS = `id, state, source, started_at, ended_at, distance_m, segment,
  (SELECT s.started_at FROM segment s WHERE s.trip_id = trip.id AND s.segment = trip.segment)
    AS segment_started_at,
  name, from_text, to_text, note`;
const ACTIVE = "state IN ('recording', 'paused')";

function isState(value: string): value is TripState {
  return (TRIP_STATES as readonly string[]).includes(value);
}

function isSource(value: string): value is TripSource {
  return (TRIP_SOURCES as readonly string[]).includes(value);
}

function toTrip(row: TripRow): Trip {
  // The tables only hold these values (schema.ts); a file that holds others is not ours to guess at.
  if (!isState(row.state) || !isSource(row.source)) {
    throw new Error(`trip ${row.id} has a state or source this version does not know`);
  }
  return {
    id: row.id,
    state: row.state,
    source: row.source,
    startedAt: row.started_at,
    endedAt: row.ended_at,
    distanceM: row.distance_m,
    segment: row.segment,
    segmentStartedAt: row.segment_started_at ?? row.started_at,
    name: row.name,
    from: row.from_text,
    to: row.to_text,
    note: row.note,
  };
}

function toPoint(row: PointRow): StoredPoint {
  return {
    tripId: row.trip_id,
    seq: row.seq,
    segment: row.segment,
    t: row.t,
    lat: row.lat,
    lon: row.lon,
    accuracyM: row.accuracy_m,
    speedMps: row.speed_mps,
  };
}

/** A typed text of the driver: trimmed, and `null` when nothing is left (`details.ts`). */
const text = cleanText;

export function createSqlStore(db: SqlDb, options: SqlStoreOptions): TripRepository {
  const thinning = options.thinning ?? RECORDING_THINNING;

  const readTrip = async (q: SqlTx, tripId: string): Promise<Trip | null> => {
    const row = await q.one<TripRow>(`SELECT ${TRIP_COLUMNS} FROM trip WHERE id = ?`, [tripId]);
    return row ? toTrip(row) : null;
  };

  const mustRead = async (q: SqlTx, tripId: string): Promise<Trip> => {
    const trip = await readTrip(q, tripId);
    if (!trip) throw new TripNotFound(tripId);
    return trip;
  };

  const readPoints = async (q: SqlTx, tripId: string): Promise<StoredPoint[]> =>
    (
      await q.all<PointRow>(
        "SELECT trip_id, seq, segment, t, lat, lon, accuracy_m, speed_mps FROM point WHERE trip_id = ? ORDER BY seq",
        [tripId],
      )
    ).map(toPoint);

  /** The segments of a trip: their starts by segment number, and their ends (`null` while open). */
  const readSegments = async (q: SqlTx, tripId: string): Promise<SegmentTimes[]> =>
    (
      await q.all<{ segment: number; started_at: number; ended_at: number | null }>(
        "SELECT segment, started_at, ended_at FROM segment WHERE trip_id = ? ORDER BY segment",
        [tripId],
      )
    ).map((s) => ({ segment: s.segment, startedAt: s.started_at, endedAt: s.ended_at }));

  const readStarts = async (q: SqlTx, tripId: string): Promise<Map<number, number>> =>
    new Map((await readSegments(q, tripId)).map((s) => [s.segment, s.startedAt]));

  /** Ends the open segment of a trip at `at` (not before its start); a closed one is left as it is. */
  const endSegment = (q: SqlTx, trip: Trip, at: number) =>
    q.run(
      "UPDATE segment SET ended_at = MAX(started_at, ?) WHERE trip_id = ? AND segment = ? AND ended_at IS NULL",
      [at, trip.id, trip.segment],
    );

  return {
    startTrip: (at) =>
      db.transaction(async (tx) => {
        const active = await tx.one<{ id: string }>(`SELECT id FROM trip WHERE ${ACTIVE}`);
        if (active) throw new TripAlreadyActive(active.id);
        const id = options.newId();
        await tx.run(
          "INSERT INTO trip (id, state, source, started_at, segment) VALUES (?, 'recording', 'recorded', ?, 1)",
          [id, at],
        );
        await tx.run("INSERT INTO segment (trip_id, segment, started_at) VALUES (?, 1, ?)", [
          id,
          at,
        ]);
        return mustRead(tx, id);
      }),

    activeTrip: async () => {
      const row = await db.one<TripRow>(`SELECT ${TRIP_COLUMNS} FROM trip WHERE ${ACTIVE}`);
      return row ? toTrip(row) : null;
    },

    appendFixes: async (tripId, fixes: readonly NewFix[]) => {
      if (fixes.length === 0) return 0;
      return db.transaction(async (tx) => {
        const trip = await readTrip(tx, tripId);
        // A delivery that arrives after Pause or Stop, or for a deleted trip, is dropped quietly.
        if (!trip || trip.state !== "recording") return 0;
        // The last stored fix is read in the same transaction as the write, so two runs of the
        // location task cannot both build on the same one.
        const last = await tx.one<{ seq: number; t: number; lat: number; lon: number }>(
          "SELECT seq, t, lat, lon FROM point WHERE trip_id = ? ORDER BY seq DESC LIMIT 1",
          [tripId],
        );
        const kept = acceptFixes(last ?? null, trip.segmentStartedAt, fixes, thinning);
        let seq = (last?.seq ?? 0) + 1;
        for (const fix of kept) {
          await tx.run(
            "INSERT INTO point (trip_id, seq, segment, t, lat, lon, accuracy_m, speed_mps) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            [tripId, seq, trip.segment, fix.t, fix.lat, fix.lon, fix.accuracyM, fix.speedMps],
          );
          seq += 1;
        }
        return kept.length;
      });
    },

    pauseTrip: (tripId, at) =>
      db.transaction(async (tx) => {
        const trip = await mustRead(tx, tripId);
        if (trip.state !== "recording") throw new TripStateError(tripId, trip.state, "paused");
        await tx.run("UPDATE trip SET state = 'paused' WHERE id = ?", [tripId]);
        await endSegment(tx, trip, at);
        return mustRead(tx, tripId);
      }),

    resumeTrip: (tripId, at) =>
      db.transaction(async (tx) => {
        const trip = await mustRead(tx, tripId);
        if (trip.state !== "paused") throw new TripStateError(tripId, trip.state, "resumed");
        const segment = trip.segment + 1;
        // A clock set back during the pause must not start a segment before the one before it.
        const startedAt = Math.max(at, trip.segmentStartedAt);
        await tx.run("UPDATE trip SET state = 'recording', segment = ? WHERE id = ?", [
          segment,
          tripId,
        ]);
        await tx.run("INSERT INTO segment (trip_id, segment, started_at) VALUES (?, ?, ?)", [
          tripId,
          segment,
          startedAt,
        ]);
        return mustRead(tx, tripId);
      }),

    stopTrip: (tripId, at, details) =>
      db.transaction(async (tx) => {
        const trip = await mustRead(tx, tripId);
        if (trip.state === "ended") throw new TripStateError(tripId, trip.state, "stopped");
        // Checked before anything is written: a detail that is too long stops nothing.
        const typed = details === undefined ? null : cleanDetails(details);
        const { distanceM } = analyseRecorded(
          await readPoints(tx, tripId),
          await readStarts(tx, tripId),
          trip.startedAt,
        );
        // A clock set back during the trip must not leave an end before the start.
        const endedAt = Math.max(at, trip.startedAt);
        await tx.run("UPDATE trip SET state = 'ended', ended_at = ?, distance_m = ? WHERE id = ?", [
          endedAt,
          distanceM,
          tripId,
        ]);
        // A trip stopped while it records ends its segment now; one stopped from a pause has it
        // ended at the Pause already.
        await endSegment(tx, trip, endedAt);
        // The details typed at Stop, in the same transaction: a trip is never ended without them.
        if (typed !== null) {
          await tx.run(
            "UPDATE trip SET name = ?, from_text = ?, to_text = ?, note = ? WHERE id = ?",
            [typed.name, typed.from, typed.to, typed.note, tripId],
          );
        }
        return mustRead(tx, tripId);
      }),

    addManualTrip: (input: ManualTrip) => {
      const { startedAt, endedAt, distanceM } = input;
      if (!Number.isFinite(startedAt) || !Number.isFinite(endedAt) || endedAt < startedAt) {
        return Promise.reject(new RangeError("The end of the trip must not be before its start"));
      }
      if (!Number.isFinite(distanceM) || distanceM < 0) {
        return Promise.reject(new RangeError("The distance must be 0 or more"));
      }
      return db.transaction(async (tx) => {
        const id = options.newId();
        await tx.run(
          "INSERT INTO trip (id, state, source, started_at, ended_at, distance_m, segment, from_text, to_text, note) VALUES (?, 'ended', 'manual', ?, ?, ?, 1, ?, ?, ?)",
          [id, startedAt, endedAt, distanceM, text(input.from), text(input.to), text(input.note)],
        );
        return mustRead(tx, id);
      });
    },

    getTrip: (tripId) => readTrip(db, tripId),

    listTrips: async (range = {}) => {
      const where: string[] = [];
      const params: number[] = [];
      if (range.from !== undefined) {
        where.push("started_at >= ?");
        params.push(range.from);
      }
      if (range.to !== undefined) {
        where.push("started_at < ?");
        params.push(range.to);
      }
      const filter = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";
      const rows = await db.all<TripRow>(
        `SELECT ${TRIP_COLUMNS} FROM trip ${filter} ORDER BY started_at DESC, id DESC`,
        params,
      );
      return rows.map(toTrip);
    },

    points: (tripId) => readPoints(db, tripId),

    progress: (tripId) =>
      db.transaction(async (tx) => {
        const trip = await readTrip(tx, tripId);
        if (!trip) return null;
        return progressOf(trip, await readPoints(tx, tripId), await readSegments(tx, tripId));
      }),

    deleteTrip: (tripId) =>
      db.transaction(async (tx) => {
        const trip = await mustRead(tx, tripId);
        if (trip.state !== "ended") throw new TripStateError(tripId, trip.state, "deleted");
        // The points and the segments go with it (ON DELETE CASCADE, schema.ts).
        await tx.run("DELETE FROM trip WHERE id = ?", [tripId]);
      }),
  };
}

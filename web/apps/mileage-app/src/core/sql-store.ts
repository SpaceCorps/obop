import type { SqlDb, SqlTx, SqlValue } from "./sql.ts";
import { TripAlreadyActive, type OutboxStore, type TripPatch } from "./store.ts";
import {
  MAX_POINTS_PER_TRIP,
  PURPOSES,
  isServerState,
  type Batch,
  type LocalTrip,
  type NewFix,
  type Problem,
  type Purpose,
  type RemoteTrip,
  type StoredPoint,
} from "./types.ts";
import { plausible, toStored } from "./values.ts";

/** Bump with a migration step in `migrate` when a table changes. */
export const SCHEMA_VERSION = 1;

const ACTIVE = "active_trip";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS trip (
  client_trip_id TEXT PRIMARY KEY NOT NULL,
  server_id TEXT,
  origin TEXT NOT NULL DEFAULT 'device',
  purpose TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  create_attempted INTEGER NOT NULL DEFAULT 0,
  stop_requested INTEGER NOT NULL DEFAULT 0,
  expected_points INTEGER,
  last_seq INTEGER,
  end_done INTEGER NOT NULL DEFAULT 0,
  submit_requested INTEGER NOT NULL DEFAULT 0,
  submit_done INTEGER NOT NULL DEFAULT 0,
  discard_reason TEXT,
  discard_done INTEGER NOT NULL DEFAULT 0,
  server_state TEXT,
  distance_m TEXT,
  integrity TEXT,
  reject_reason TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  problem_step TEXT,
  problem_code TEXT,
  problem_message TEXT,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS point (
  client_trip_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  t INTEGER NOT NULL,
  lat TEXT NOT NULL,
  lon TEXT NOT NULL,
  accuracy_m TEXT,
  speed_mps TEXT,
  PRIMARY KEY (client_trip_id, seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS batch (
  client_trip_id TEXT NOT NULL,
  first_seq INTEGER NOT NULL,
  last_seq INTEGER NOT NULL,
  sent INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (client_trip_id, first_seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
) WITHOUT ROWID;
`;

/** Creates the tables on a new database. A database of a newer app is refused, not changed. */
export async function migrate(db: SqlDb): Promise<void> {
  const row = await db.one<{ user_version: number }>("PRAGMA user_version");
  const version = Number(row?.user_version ?? 0);
  if (version > SCHEMA_VERSION) {
    throw new Error(`The database is from a newer version of the app (schema ${version})`);
  }
  if (version < 1) {
    await db.transaction(async (tx) => {
      await tx.exec(SCHEMA);
      await tx.exec(`PRAGMA user_version = ${SCHEMA_VERSION}`);
    });
  }
}

interface TripRow {
  client_trip_id: string;
  server_id: string | null;
  origin: string;
  purpose: string;
  started_at: string;
  ended_at: string | null;
  create_attempted: number;
  stop_requested: number;
  expected_points: number | null;
  last_seq: number | null;
  end_done: number;
  submit_requested: number;
  submit_done: number;
  discard_reason: string | null;
  discard_done: number;
  server_state: string | null;
  distance_m: string | null;
  integrity: string | null;
  reject_reason: string | null;
  attempts: number;
  problem_step: string | null;
  problem_code: string | null;
  problem_message: string | null;
  created_ms: number;
}

const STEPS: readonly Problem["step"][] = ["create", "points", "end", "submit", "discard"];

function toTrip(r: TripRow): LocalTrip {
  const step = STEPS.find((s) => s === r.problem_step);
  return {
    clientTripId: r.client_trip_id,
    serverId: r.server_id,
    origin: r.origin === "server" ? "server" : "device",
    purpose: (PURPOSES as readonly string[]).includes(r.purpose)
      ? (r.purpose as Purpose)
      : "business",
    startedAt: r.started_at,
    endedAt: r.ended_at,
    createAttempted: r.create_attempted === 1,
    stopRequested: r.stop_requested === 1,
    expectedPoints: r.expected_points,
    lastSeq: r.last_seq,
    endDone: r.end_done === 1,
    submitRequested: r.submit_requested === 1,
    submitDone: r.submit_done === 1,
    discardReason: r.discard_reason,
    discardDone: r.discard_done === 1,
    serverState: isServerState(r.server_state) ? r.server_state : null,
    distanceM: r.distance_m,
    integrity: r.integrity,
    rejectReason: r.reject_reason,
    attempts: r.attempts,
    problem:
      step !== undefined && r.problem_code !== null
        ? { step, code: r.problem_code, message: r.problem_message ?? "" }
        : null,
    createdMs: r.created_ms,
  };
}

interface PointRow {
  seq: number;
  t: number;
  lat: string;
  lon: string;
  accuracy_m: string | null;
  speed_mps: string | null;
}

const toPoint = (r: PointRow): StoredPoint => ({
  seq: r.seq,
  t: r.t,
  lat: r.lat,
  lon: r.lon,
  accuracyM: r.accuracy_m,
  speedMps: r.speed_mps,
});

const flag = (v: boolean): number => (v ? 1 : 0);

/** The column and the value of each key of a patch. */
function assignments(patch: TripPatch): { sets: string[]; values: SqlValue[] } {
  const sets: string[] = [];
  const values: SqlValue[] = [];
  const add = (column: string, value: SqlValue | undefined) => {
    if (value === undefined) return;
    sets.push(`${column} = ?`);
    values.push(value);
  };
  add("server_id", patch.serverId);
  add("ended_at", patch.endedAt);
  add(
    "create_attempted",
    patch.createAttempted === undefined ? undefined : flag(patch.createAttempted),
  );
  add("end_done", patch.endDone === undefined ? undefined : flag(patch.endDone));
  add(
    "submit_requested",
    patch.submitRequested === undefined ? undefined : flag(patch.submitRequested),
  );
  add("submit_done", patch.submitDone === undefined ? undefined : flag(patch.submitDone));
  add("discard_done", patch.discardDone === undefined ? undefined : flag(patch.discardDone));
  add("server_state", patch.serverState);
  add("distance_m", patch.distanceM);
  add("integrity", patch.integrity);
  add("reject_reason", patch.rejectReason);
  add("attempts", patch.attempts);
  if (patch.problem !== undefined) {
    add("problem_step", patch.problem?.step ?? null);
    add("problem_code", patch.problem?.code ?? null);
    add("problem_message", patch.problem?.message ?? null);
  }
  return { sets, values };
}

async function tripOf(tx: SqlTx, clientTripId: string): Promise<LocalTrip | null> {
  const row = await tx.one<TripRow>("SELECT * FROM trip WHERE client_trip_id = ?", [clientTripId]);
  return row ? toTrip(row) : null;
}

async function setMeta(tx: SqlTx, key: string, value: string | null): Promise<void> {
  if (value === null) await tx.run("DELETE FROM meta WHERE key = ?", [key]);
  else
    await tx.run(
      "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      [key, value],
    );
}

async function getMeta(tx: SqlTx, key: string): Promise<string | null> {
  return (
    (await tx.one<{ value: string }>("SELECT value FROM meta WHERE key = ?", [key]))?.value ?? null
  );
}

async function clearActive(tx: SqlTx, clientTripId: string): Promise<void> {
  if ((await getMeta(tx, ACTIVE)) === clientTripId) await setMeta(tx, ACTIVE, null);
}

/** The outbox over a SQL database. Run `migrate` on the database first. */
export function createSqlStore(db: SqlDb): OutboxStore {
  return {
    async startTrip({ clientTripId, purpose, startedAt, nowMs }) {
      return db.transaction(async (tx) => {
        const active = await getMeta(tx, ACTIVE);
        if (active !== null) {
          const current = await tripOf(tx, active);
          if (current && !current.stopRequested && current.discardReason === null) {
            throw new TripAlreadyActive(active);
          }
        }
        await tx.run(
          "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES (?, ?, ?, ?)",
          [clientTripId, purpose, startedAt, nowMs],
        );
        await setMeta(tx, ACTIVE, clientTripId);
        return (await tripOf(tx, clientTripId))!;
      });
    },

    async activeTrip() {
      const id = await getMeta(db, ACTIVE);
      if (id === null) return null;
      const trip = await tripOf(db, id);
      if (!trip || trip.stopRequested || trip.discardReason !== null) {
        await db.transaction(async (tx) => {
          if ((await getMeta(tx, ACTIVE)) === id) await setMeta(tx, ACTIVE, null);
        });
        return null;
      }
      return trip;
    },

    async getTrip(clientTripId) {
      const row = await db.one<TripRow>("SELECT * FROM trip WHERE client_trip_id = ?", [
        clientTripId,
      ]);
      return row ? toTrip(row) : null;
    },

    async listTrips() {
      const rows = await db.all<TripRow>(
        "SELECT * FROM trip ORDER BY started_at DESC, created_ms DESC",
      );
      return rows.map(toTrip);
    },

    async appendFixes(clientTripId, fixes) {
      if (fixes.length === 0) return 0;
      return db.transaction(async (tx) => {
        const trip = await tripOf(tx, clientTripId);
        if (!trip || trip.stopRequested || trip.discardReason !== null) return 0;
        const last = await tx.one<{ seq: number; t: number }>(
          "SELECT seq, t FROM point WHERE client_trip_id = ? ORDER BY seq DESC LIMIT 1",
          [clientTripId],
        );
        let seq = (last?.seq ?? 0) + 1;
        let lastT = last?.t ?? Number.NEGATIVE_INFINITY;
        let stored = 0;
        const ordered: NewFix[] = [...fixes].sort((a, b) => a.t - b.t);
        for (const fix of ordered) {
          if (!plausible(fix) || fix.t <= lastT) continue;
          if (seq > MAX_POINTS_PER_TRIP) break;
          const p = toStored(seq, fix);
          await tx.run(
            "INSERT INTO point (client_trip_id, seq, t, lat, lon, accuracy_m, speed_mps) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [clientTripId, p.seq, p.t, p.lat, p.lon, p.accuracyM, p.speedMps],
          );
          lastT = fix.t;
          seq++;
          stored++;
        }
        return stored;
      });
    },

    async stopTrip(clientTripId, endedAt) {
      return db.transaction(async (tx) => {
        const trip = await tripOf(tx, clientTripId);
        if (!trip) throw new Error(`No trip ${clientTripId}`);
        if (!trip.stopRequested) {
          const counts = await tx.one<{ n: number; top: number }>(
            "SELECT COUNT(*) AS n, COALESCE(MAX(seq), 0) AS top FROM point WHERE client_trip_id = ?",
            [clientTripId],
          );
          await tx.run(
            "UPDATE trip SET stop_requested = 1, ended_at = ?, expected_points = ?, last_seq = ?, attempts = 0 WHERE client_trip_id = ?",
            [endedAt, counts?.n ?? 0, counts?.top ?? 0, clientTripId],
          );
        }
        await clearActive(tx, clientTripId);
        return (await tripOf(tx, clientTripId))!;
      });
    },

    async requestSubmit(clientTripId) {
      await db.run(
        "UPDATE trip SET submit_requested = 1, problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ? AND submit_done = 0",
        [clientTripId],
      );
    },

    async requestDiscard(clientTripId, reason, endedAt) {
      await db.transaction(async (tx) => {
        const trip = await tripOf(tx, clientTripId);
        if (!trip || trip.discardReason !== null) return;
        await tx.run(
          "UPDATE trip SET discard_reason = ?, stop_requested = 1, ended_at = COALESCE(ended_at, ?), problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ?",
          [reason, endedAt, clientTripId],
        );
        await clearActive(tx, clientTripId);
      });
    },

    async clearProblem(clientTripId) {
      await db.run(
        "UPDATE trip SET problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ?",
        [clientTripId],
      );
    },

    async patchTrip(clientTripId, patch) {
      const { sets, values } = assignments(patch);
      if (sets.length === 0) return;
      await db.run(`UPDATE trip SET ${sets.join(", ")} WHERE client_trip_id = ?`, [
        ...values,
        clientTripId,
      ]);
    },

    async mergeRemote(trips: readonly RemoteTrip[]) {
      if (trips.length === 0) return;
      await db.transaction(async (tx) => {
        for (const r of trips) {
          const known = await tripOf(tx, r.clientTripId);
          if (known) {
            await tx.run(
              "UPDATE trip SET server_id = COALESCE(server_id, ?), server_state = ?, distance_m = COALESCE(?, distance_m), integrity = COALESCE(?, integrity), reject_reason = ? WHERE client_trip_id = ?",
              [r.id, r.state, r.distanceM, r.integrity, r.rejectReason, r.clientTripId],
            );
            continue;
          }
          await tx.run(
            `INSERT INTO trip (client_trip_id, server_id, origin, purpose, started_at, ended_at,
               create_attempted, stop_requested, end_done, submit_requested, submit_done, discard_done,
               server_state, distance_m, integrity, reject_reason, created_ms)
             VALUES (?, ?, 'server', ?, ?, ?, 1, 1, ?, 0, ?, ?, ?, ?, ?, ?, ?)`,
            [
              r.clientTripId,
              r.id,
              r.purpose ?? "business",
              r.startedAt ?? new Date(0).toISOString(),
              r.endedAt,
              flag(r.state !== "recording"),
              flag(r.state === "in_review" || r.state === "confirmed"),
              flag(r.state === "discarded"),
              r.state,
              r.distanceM,
              r.integrity,
              r.rejectReason,
              Date.parse(r.startedAt ?? "") || 0,
            ],
          );
        }
      });
    },

    async points(clientTripId) {
      const rows = await db.all<PointRow>(
        "SELECT seq, t, lat, lon, accuracy_m, speed_mps FROM point WHERE client_trip_id = ? ORDER BY seq",
        [clientTripId],
      );
      return rows.map(toPoint);
    },

    async pointCount(clientTripId) {
      const row = await db.one<{ n: number }>(
        "SELECT COUNT(*) AS n FROM point WHERE client_trip_id = ?",
        [clientTripId],
      );
      return row?.n ?? 0;
    },

    async nextBatch(clientTripId, max) {
      return db.transaction(async (tx) => {
        const open = await tx.one<{ first_seq: number; last_seq: number }>(
          "SELECT first_seq, last_seq FROM batch WHERE client_trip_id = ? AND sent = 0 ORDER BY first_seq LIMIT 1",
          [clientTripId],
        );
        if (open) return { clientTripId, firstSeq: open.first_seq, lastSeq: open.last_seq };
        const after = await tx.one<{ top: number }>(
          "SELECT COALESCE(MAX(last_seq), 0) AS top FROM batch WHERE client_trip_id = ?",
          [clientTripId],
        );
        const range = await tx.one<{ first: number | null; last: number | null }>(
          `SELECT MIN(seq) AS first, MAX(seq) AS last FROM
             (SELECT seq FROM point WHERE client_trip_id = ? AND seq > ? ORDER BY seq LIMIT ?)`,
          [clientTripId, after?.top ?? 0, max],
        );
        if (range?.first == null || range.last == null) return null;
        await tx.run(
          "INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES (?, ?, ?, 0)",
          [clientTripId, range.first, range.last],
        );
        return { clientTripId, firstSeq: range.first, lastSeq: range.last };
      });
    },

    async batchPoints(batch: Batch) {
      const rows = await db.all<PointRow>(
        "SELECT seq, t, lat, lon, accuracy_m, speed_mps FROM point WHERE client_trip_id = ? AND seq BETWEEN ? AND ? ORDER BY seq",
        [batch.clientTripId, batch.firstSeq, batch.lastSeq],
      );
      return rows.map(toPoint);
    },

    async markBatchSent(batch) {
      await db.run("UPDATE batch SET sent = 1 WHERE client_trip_id = ? AND first_seq = ?", [
        batch.clientTripId,
        batch.firstSeq,
      ]);
    },

    async purgePoints(clientTripId) {
      await db.transaction(async (tx) => {
        await tx.run("DELETE FROM point WHERE client_trip_id = ?", [clientTripId]);
        await tx.run("DELETE FROM batch WHERE client_trip_id = ?", [clientTripId]);
      });
    },

    async deleteTrip(clientTripId) {
      await db.transaction(async (tx) => {
        await tx.run("DELETE FROM point WHERE client_trip_id = ?", [clientTripId]);
        await tx.run("DELETE FROM batch WHERE client_trip_id = ?", [clientTripId]);
        await tx.run("DELETE FROM trip WHERE client_trip_id = ?", [clientTripId]);
        await clearActive(tx, clientTripId);
      });
    },

    async clearAll() {
      await db.transaction(async (tx) => {
        await tx.run("DELETE FROM point", []);
        await tx.run("DELETE FROM batch", []);
        await tx.run("DELETE FROM trip", []);
        await setMeta(tx, ACTIVE, null);
      });
    },

    async getMeta(key) {
      return (
        (await db.one<{ value: string }>("SELECT value FROM meta WHERE key = ?", [key]))?.value ??
        null
      );
    },

    async setMeta(key, value) {
      await db.transaction((tx) => setMeta(tx, key, value));
    },
  };
}

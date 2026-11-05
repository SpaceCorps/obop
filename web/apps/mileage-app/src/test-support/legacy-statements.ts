/**
 * The SQL that the first build ran against the database file, kept as it was. Each statement is a
 * copy, word for word, of a string in `core/sql-store.ts` of the commit before the schema manager
 * (`89c4e503`, the store that the first builds shipped). `patchTrip` builds its statement from a
 * list of columns; the copy is that statement with every column it can set. A copy, not an import:
 * `core/sql-store.ts` is code that moves on, and the point of this file is the statements that an
 * installed phone keeps running after it has been updated and then downgraded. The older-build probe
 * (`older-build-probe.ts`) runs them, and `schema-upgrade.test.ts` holds the text to a checksum.
 * Nothing in it may change.
 */

export const FIRST_BUILD_SQL = {
  tripById: "SELECT * FROM trip WHERE client_trip_id = ?",
  listTrips: "SELECT * FROM trip ORDER BY started_at DESC, created_ms DESC",
  insertTrip:
    "INSERT INTO trip (client_trip_id, purpose, started_at, created_ms) VALUES (?, ?, ?, ?)",
  stopTrip:
    "UPDATE trip SET stop_requested = 1, ended_at = ?, expected_points = ?, last_seq = ?, attempts = 0 WHERE client_trip_id = ?",
  requestSubmit:
    "UPDATE trip SET submit_requested = 1, problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ? AND submit_done = 0",
  requestDiscard:
    "UPDATE trip SET discard_reason = ?, stop_requested = 1, ended_at = COALESCE(ended_at, ?), problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ?",
  clearProblem:
    "UPDATE trip SET problem_step = NULL, problem_code = NULL, problem_message = NULL, attempts = 0 WHERE client_trip_id = ?",
  patchTrip:
    "UPDATE trip SET server_id = ?, ended_at = ?, create_attempted = ?, end_done = ?, submit_requested = ?, submit_done = ?, discard_done = ?, server_state = ?, distance_m = ?, integrity = ?, reject_reason = ?, attempts = ?, problem_step = ?, problem_code = ?, problem_message = ? WHERE client_trip_id = ?",
  mergeUpdate:
    "UPDATE trip SET server_id = COALESCE(server_id, ?), server_state = ?, distance_m = COALESCE(?, distance_m), integrity = COALESCE(?, integrity), reject_reason = ? WHERE client_trip_id = ?",
  mergeInsert: `INSERT INTO trip (client_trip_id, server_id, origin, purpose, started_at, ended_at,
               create_attempted, stop_requested, end_done, submit_requested, submit_done, discard_done,
               server_state, distance_m, integrity, reject_reason, created_ms)
             VALUES (?, ?, 'server', ?, ?, ?, 1, 1, ?, 0, ?, ?, ?, ?, ?, ?, ?)`,
  deleteTrip: "DELETE FROM trip WHERE client_trip_id = ?",
  clearTrips: "DELETE FROM trip",

  lastPoint:
    "SELECT seq, t, lat, lon FROM point WHERE client_trip_id = ? ORDER BY seq DESC LIMIT 1",
  insertPoint:
    "INSERT INTO point (client_trip_id, seq, t, lat, lon, accuracy_m, speed_mps) VALUES (?, ?, ?, ?, ?, ?, ?)",
  countAndTop:
    "SELECT COUNT(*) AS n, COALESCE(MAX(seq), 0) AS top FROM point WHERE client_trip_id = ?",
  points:
    "SELECT seq, t, lat, lon, accuracy_m, speed_mps FROM point WHERE client_trip_id = ? ORDER BY seq",
  pointCount: "SELECT COUNT(*) AS n FROM point WHERE client_trip_id = ?",
  batchPoints:
    "SELECT seq, t, lat, lon, accuracy_m, speed_mps FROM point WHERE client_trip_id = ? AND seq BETWEEN ? AND ? ORDER BY seq",
  deletePointsOf: "DELETE FROM point WHERE client_trip_id = ?",
  clearPoints: "DELETE FROM point",

  openBatch:
    "SELECT first_seq, last_seq FROM batch WHERE client_trip_id = ? AND sent = 0 ORDER BY first_seq LIMIT 1",
  batchTop: "SELECT COALESCE(MAX(last_seq), 0) AS top FROM batch WHERE client_trip_id = ?",
  nextRange: `SELECT MIN(seq) AS first, MAX(seq) AS last FROM
             (SELECT seq FROM point WHERE client_trip_id = ? AND seq > ? ORDER BY seq LIMIT ?)`,
  insertBatch: "INSERT INTO batch (client_trip_id, first_seq, last_seq, sent) VALUES (?, ?, ?, 0)",
  markBatchSent: "UPDATE batch SET sent = 1 WHERE client_trip_id = ? AND first_seq = ?",
  deleteBatchesOf: "DELETE FROM batch WHERE client_trip_id = ?",
  clearBatches: "DELETE FROM batch",

  selectMeta: "SELECT value FROM meta WHERE key = ?",
  upsertMeta:
    "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
  deleteMeta: "DELETE FROM meta WHERE key = ?",
} as const;

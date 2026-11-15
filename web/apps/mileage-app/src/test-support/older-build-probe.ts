import type { SqlDb, SqlTx, SqlValue } from "../core/sql.ts";
import { SCHEMA as FIRST_LAYOUT, migrate as firstBuildMigration } from "./legacy-schema.ts";
import { FIRST_BUILD_SQL as Q } from "./legacy-statements.ts";
import { memoryDb } from "./memory-db.ts";

/**
 * Whether the first build still works on a database file: the oracle for any migration of a
 * module version 2 or later (see `docs/spec/mobile.md` 8.7.1).
 *
 * The schema manager's check of a migration (`checkChange` in `core/schema.ts`) is a lint: a list
 * of changes that its author could think of that would stop an older build. What shows that a given
 * migration is safe is this probe: it opens the file with the first build's own migration and then
 * runs the first build's own statements (`legacy-statements.ts`, copies of the store of the first
 * build) in the order of its flows: start a trip, record fixes, cut batches, stop, submit, discard,
 * patch, merge what the server says, list, keep a note, delete, clear. Each statement is checked
 * for what the first build expects of it, both ways: every column the answer has is expected and
 * every column that is expected is in the answer (a row of a trip is held to the columns of the
 * first layout, from the first build's own schema text, so a column that a later version added to
 * `trip` is in the answer of `SELECT *` and is not compared, because the first build reads a row by
 * the names it knows), and every value is compared as JavaScript sees it: text, number, NULL and
 * blob are told apart, an integer and a real of the same value are not (and the first build,
 * which reads numbers as JavaScript numbers, does not tell them apart either; Infinity, NaN and
 * -0 are told apart from NULL, 0 and text, as `canon` tags them). The one answer that
 * is not compared whole is the list of trips: it has to hold the three trips the probe made. All of
 * it runs in a transaction that is rolled back, so the file is as it was afterwards, with its rows.
 *
 * A migration of a module version 2 comes with a test that runs the probe on the file after it. The
 * probe has no rows in a table that a migration adds (the first build never opens those), so a
 * constraint on one of those, a foreign key into the tables of the first layout and the like are
 * outside what it can show; the lint refuses them on principle, and `schema-probe.test.ts` says
 * which refusals the probe confirms and which it cannot.
 */

export type ProbeResult = { ok: true } | { ok: false; step: string; error: string };

/** Thrown out of the transaction, to roll it back with the answer. */
class Finished extends Error {
  constructor(readonly result: ProbeResult) {
    super("the probe is finished");
  }
}

type Row = Record<string, SqlValue>;

const T0 = "2026-10-15T09:00:00.000Z";
const T1 = "2026-10-15T09:30:00.000Z";
const T2 = "2026-10-15T10:00:00.000Z";

/** The columns of a trip, with the values a trip has when the first build has just recorded it. */
const newTrip = (id: string, purpose: string, startedAt: string, createdMs: number): Row => ({
  client_trip_id: id,
  server_id: null,
  origin: "device",
  purpose,
  started_at: startedAt,
  ended_at: null,
  create_attempted: 0,
  stop_requested: 0,
  expected_points: null,
  last_seq: null,
  end_done: 0,
  submit_requested: 0,
  submit_done: 0,
  discard_reason: null,
  discard_done: 0,
  server_state: null,
  distance_m: null,
  integrity: null,
  reject_reason: null,
  attempts: 0,
  problem_step: null,
  problem_code: null,
  problem_message: null,
  created_ms: createdMs,
});

/** The fixes of a trip as the first build stores them: coordinates are text. */
const fixes = (): Row[] =>
  [1, 2, 3].map((seq) => ({
    seq,
    t: 1_000 * seq,
    lat: `60.17${seq}`,
    lon: `24.94${seq}`,
    accuracy_m: seq === 2 ? "4.5" : null,
    speed_mps: null,
  }));

/**
 * JSON with the keys of every object in order, so that two rows are the same when their keys and
 * their values are, as JavaScript sees a value: text, number, NULL and blob are different things.
 * JSON writes Infinity, -Infinity and NaN as null and -0 as 0, so those are tagged first: a real
 * that comes back as one of them (`SELECT 9e999` is Infinity) is not the NULL or the 0 that was
 * expected, and neither is a whole number that comes back as a BigInt.
 */
const canon = (value: unknown): string =>
  JSON.stringify(value, (_, v: unknown) => {
    if (typeof v === "number" && (!Number.isFinite(v) || Object.is(v, -0))) {
      return { $number: Object.is(v, -0) ? "-0" : String(v) };
    }
    if (typeof v === "bigint") return { $bigint: String(v) };
    return v !== null && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : v;
  });

/** The columns of `trip` in the first layout, from the text the first build made the table with. */
let tripColumnsOnce: Promise<string[]> | undefined;
const firstLayoutTripColumns = (): Promise<string[]> => {
  tripColumnsOnce ??= (async () => {
    const db = memoryDb();
    await db.exec(FIRST_LAYOUT);
    return (await db.all<{ name: string }>("PRAGMA table_info(trip)")).map((c) => c.name);
  })();
  return tripColumnsOnce;
};

/** The columns that the statement of the first build sets, from its text: `SET a = ?, b = ? WHERE`. */
const PATCHED: readonly string[] = (() => {
  const set = /^UPDATE trip SET (.*) WHERE client_trip_id = \?$/.exec(Q.patchTrip)?.[1];
  const columns = (set ?? "").split(", ").map((part) => /^(\w+) = \?$/.exec(part)?.[1] ?? "");
  if (set === undefined || columns.includes("")) {
    throw new Error("the statement patchTrip is not a list of `column = ?`");
  }
  return columns;
})();

/**
 * Holds an answer to `columns` both ways: the row the probe expects has exactly those columns (a
 * probe that forgot one would not look at it, and one that has an extra would not look at that),
 * and the answer has every one of them, with the value and the kind of value expected. A column
 * of the answer that is not in `columns` is not looked at: it is one that a later version of the
 * app added to `trip`, and the first build reads a row by the names it knows. Exported for its tests.
 */
export function expectRow(
  what: string,
  columns: readonly string[],
  row: Row | null,
  expected: Row,
): void {
  const named = Object.keys(expected);
  for (const column of columns) {
    if (!named.includes(column)) {
      throw new Error(`${what}: the probe expects no value for the column ${column}`);
    }
  }
  for (const column of named) {
    if (!columns.includes(column)) {
      throw new Error(
        `${what}: the probe expects a column ${column}, which the first layout lacks`,
      );
    }
  }
  if (row === null) throw new Error(`${what}: expected a row, got null`);
  for (const column of columns) {
    if (!(column in row)) throw new Error(`${what}: the column ${column} is gone`);
    const got = canon(row[column]);
    const want = canon(expected[column]);
    if (got !== want) throw new Error(`${what}: the column ${column} is ${got}, expected ${want}`);
  }
}

function expectSame(what: string, actual: unknown, expected: unknown): void {
  const a = canon(actual);
  const b = canon(expected);
  if (a !== b) throw new Error(`${what}: expected ${b}, got ${a}`);
}

async function expectRefused(what: string, action: Promise<unknown>): Promise<void> {
  try {
    await action;
  } catch {
    return;
  }
  throw new Error(`${what}: the file accepted it`);
}

/** The steps of the first build's flows, against one transaction. */
function steps(tx: SqlTx): [string, () => Promise<void>][] {
  const A = "probe-a";
  const B = "probe-b";
  const C = "probe-c";
  const run = (sql: string, params: SqlValue[] = []) => tx.run(sql, params);
  const one = (sql: string, params: SqlValue[] = []) => tx.one<Row>(sql, params);
  const all = (sql: string, params: SqlValue[] = []) => tx.all<Row>(sql, params);
  /** The trip with this id as `SELECT *` gives it, held to the columns of the first layout. */
  const expectTrip = async (what: string, id: string, expected: Row) =>
    expectRow(what, await firstLayoutTripColumns(), await one(Q.tripById, [id]), expected);
  const pointsOf = async (id: string) => all(Q.points, [id]);
  const stopped: Row = {
    ...newTrip(A, "business", T0, 1),
    stop_requested: 1,
    ended_at: T1,
    expected_points: 3,
    last_seq: 3,
  };
  const submitted: Row = { ...stopped, submit_requested: 1 };
  /** What `patchTrip` is given, one value for each column it sets, and nothing else. */
  const set: Row = {
    server_id: "s-1",
    ended_at: T2,
    create_attempted: 1,
    end_done: 1,
    submit_requested: 1,
    submit_done: 1,
    discard_done: 1,
    server_state: "in_review",
    distance_m: "12.5",
    integrity: "ok",
    reject_reason: "none",
    attempts: 2,
    problem_step: "points",
    problem_code: "offline",
    problem_message: "No network",
  };
  const patched: Row = { ...submitted, ...set };
  const cleared: Row = {
    ...patched,
    problem_step: null,
    problem_code: null,
    problem_message: null,
    attempts: 0,
  };
  const merged: Row = {
    ...cleared,
    server_state: "confirmed",
    distance_m: "13.0",
    integrity: "checked",
    reject_reason: null,
  };

  return [
    [
      "start a trip",
      async () => {
        expectSame("no trip yet", await one(Q.tripById, [A]), null);
        await run(Q.insertTrip, [A, "business", T0, 1]);
        await run(Q.upsertMeta, ["active_trip", A]);
        await expectTrip("the trip", A, newTrip(A, "business", T0, 1));
        expectSame("the active trip", (await one(Q.selectMeta, ["active_trip"]))?.value, A);
      },
    ],
    [
      "start a second trip with the same values",
      async () => {
        await run(Q.insertTrip, [B, "business", T0, 1]);
        await expectTrip("the second trip", B, newTrip(B, "business", T0, 1));
      },
    ],
    [
      "refuse a trip that is recorded twice",
      async () => {
        await expectRefused("the same id again", run(Q.insertTrip, [A, "business", T0, 1]));
      },
    ],
    [
      "record fixes",
      async () => {
        for (const id of [A, B]) {
          expectSame("no fix yet", await one(Q.lastPoint, [id]), null);
          for (const f of fixes()) {
            await run(Q.insertPoint, [
              id,
              f.seq!,
              f.t!,
              f.lat!,
              f.lon!,
              f.accuracy_m!,
              f.speed_mps!,
            ]);
          }
          expectSame("the last fix", await one(Q.lastPoint, [id]), {
            seq: 3,
            t: 3_000,
            lat: "60.173",
            lon: "24.943",
          });
          expectSame("the fixes", await pointsOf(id), fixes());
        }
        await expectRefused(
          "the same fix again",
          run(Q.insertPoint, [A, 1, 1_000, "60.171", "24.941", null, null]),
        );
      },
    ],
    [
      "count the fixes",
      async () => {
        expectSame("the count", await one(Q.countAndTop, [A]), { n: 3, top: 3 });
        // The count of a trip with fixes is read again at the end of the flow, after the writes
        // in between ("delete a trip"); here, straight after the fixes were recorded and read
        // back, nothing could have changed it, so it is not asked for twice.
        expectSame("the count of a trip without", await one(Q.pointCount, ["probe-none"]), {
          n: 0,
        });
      },
    ],
    [
      "stop a trip",
      async () => {
        const counts = await one(Q.countAndTop, [A]);
        await run(Q.stopTrip, [T1, counts!.n!, counts!.top!, A]);
        await run(Q.deleteMeta, ["active_trip"]);
        await expectTrip("the stopped trip", A, stopped);
        expectSame("no active trip", await one(Q.selectMeta, ["active_trip"]), null);
      },
    ],
    [
      "cut and send batches",
      async () => {
        expectSame("no open batch", await one(Q.openBatch, [A]), null);
        expectSame("nothing cut yet", await one(Q.batchTop, [A]), { top: 0 });
        expectSame("the range", await one(Q.nextRange, [A, 0, 500]), { first: 1, last: 3 });
        await run(Q.insertBatch, [A, 1, 3]);
        await run(Q.insertBatch, [B, 1, 3]);
        expectSame("the open batch", await one(Q.openBatch, [A]), { first_seq: 1, last_seq: 3 });
        expectSame("the cut", await one(Q.batchTop, [A]), { top: 3 });
        expectSame("the points of the batch", await all(Q.batchPoints, [A, 1, 3]), fixes());
        await run(Q.markBatchSent, [A, 1]);
        expectSame("no open batch after", await one(Q.openBatch, [A]), null);
        await expectRefused("the same batch again", run(Q.insertBatch, [A, 1, 3]));
      },
    ],
    [
      "submit and discard a trip",
      async () => {
        await run(Q.requestSubmit, [A]);
        await expectTrip("the submitted trip", A, submitted);
        await run(Q.requestDiscard, ["wrong trip", T2, B]);
        await expectTrip("the discarded trip", B, {
          ...newTrip(B, "business", T0, 1),
          discard_reason: "wrong trip",
          stop_requested: 1,
          ended_at: T2,
        });
      },
    ],
    [
      "patch a trip with every column it can set, then clear its problem",
      async () => {
        if (canon(Object.keys(set).sort()) !== canon([...PATCHED].sort())) {
          throw new Error("the values of the probe are not the columns that patchTrip sets");
        }
        await run(Q.patchTrip, [...PATCHED.map((column) => set[column]!), A]);
        await expectTrip("the patched trip", A, patched);
        // The problem the patch left on the trip (it is the only statement that sets one) is what
        // the statement that clears it has to clear.
        await run(Q.clearProblem, [A]);
        await expectTrip("the problem is cleared", A, cleared);
      },
    ],
    [
      "merge what the server says",
      async () => {
        await run(Q.mergeUpdate, ["s-2", "confirmed", "13.0", "checked", null, A]);
        await expectTrip("the merged trip", A, merged);
        await run(Q.mergeInsert, [
          C,
          "s-3",
          "private",
          T0,
          T1,
          1,
          1,
          0,
          "in_review",
          "3.0",
          "ok",
          null,
          5,
        ]);
        await expectTrip("the trip from the server", C, {
          ...newTrip(C, "private", T0, 5),
          server_id: "s-3",
          origin: "server",
          ended_at: T1,
          create_attempted: 1,
          stop_requested: 1,
          end_done: 1,
          submit_done: 1,
          discard_done: 0,
          server_state: "in_review",
          distance_m: "3.0",
          integrity: "ok",
        });
      },
    ],
    [
      "list the trips",
      async () => {
        const ids = (await all(Q.listTrips)).map((r) => r.client_trip_id);
        for (const id of [A, B, C]) {
          if (!ids.includes(id)) throw new Error(`the list does not have ${id}`);
        }
      },
    ],
    [
      "keep a note",
      async () => {
        await run(Q.upsertMeta, ["probe-note", "one"]);
        await run(Q.upsertMeta, ["probe-note", "two"]);
        expectSame("the note", (await one(Q.selectMeta, ["probe-note"]))?.value, "two");
        await run(Q.deleteMeta, ["probe-note"]);
        expectSame("the note is gone", await one(Q.selectMeta, ["probe-note"]), null);
      },
    ],
    [
      "delete a trip",
      async () => {
        await run(Q.deletePointsOf, [A]);
        await run(Q.deleteBatchesOf, [A]);
        await run(Q.deleteTrip, [A]);
        expectSame("the trip is gone", await one(Q.tripById, [A]), null);
        expectSame("its fixes are gone", await one(Q.pointCount, [A]), { n: 0 });
        expectSame("the other trip is not", (await one(Q.pointCount, [B]))?.n, 3);
      },
    ],
    [
      "clear everything",
      async () => {
        await run(Q.clearPoints);
        await run(Q.clearBatches);
        await run(Q.clearTrips);
        expectSame("no trip", await all(Q.listTrips), []);
        expectSame("no fix", await one("SELECT COUNT(*) AS n FROM point"), { n: 0 });
        expectSame("no batch", await one("SELECT COUNT(*) AS n FROM batch"), { n: 0 });
      },
    ],
  ];
}

/**
 * Runs the first build against `db` and says whether everything it does works. The file is left
 * as it was. The first build's own migration comes first, inside the transaction that is rolled
 * back: on a file of layout 1 it reads the layout and does nothing, on a file of a newer layout
 * it refuses, and on a file with no layout it takes the file for a new one and makes its tables
 * (which are there, so nothing changes) and sets the layout.
 */
export async function probeOlderBuild(db: SqlDb): Promise<ProbeResult> {
  try {
    await db.transaction(async (tx) => {
      // The migration asks for a transaction of its own; the one that is open is the one it gets.
      const asDb: SqlDb = { ...tx, transaction: (fn) => fn(tx) };
      const open: [string, () => Promise<void>] = [
        "open the file",
        () => firstBuildMigration(asDb),
      ];
      for (const [step, run] of [open, ...steps(tx)]) {
        try {
          await run();
        } catch (error) {
          throw new Finished({
            ok: false,
            step,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
      throw new Finished({ ok: true });
    });
  } catch (error) {
    if (error instanceof Finished) return error.result;
    throw error;
  }
  throw new Error("the transaction of the probe ended without an answer");
}

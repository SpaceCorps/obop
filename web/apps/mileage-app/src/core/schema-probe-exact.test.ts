import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import type { SqlDb, SqlTx, SqlValue } from "./sql.ts";
import { FIRST_BUILD_SQL as Q } from "../test-support/legacy-statements.ts";
import { emptyLegacyFile } from "../test-support/legacy-file.ts";
import { expectRow, probeOlderBuild } from "../test-support/older-build-probe.ts";

// The probe says that the first build still works on a file by comparing what the file answers
// with what the first build expects of it. This file shows that the comparison is exact: every key
// of every row that any statement of the probe reads back is compared, one at a time, in both
// directions, and as the kind of value it is. It does that from the answers, not from the probe's
// own list of what it expects, so a key that the probe forgot to expect is a key whose change goes
// unnoticed, and that is what fails here.
//
// A read is altered after the file has answered it: one key of one row is taken out, given another
// value, given the same value as another kind of value (a number as text, a text as a blob, NULL as
// 0), or a key is put in that the statement does not have. The probe has to fail. The exceptions
// are few, and each one is said below with its reason and held by a test:
//
// - The list of trips is not compared whole (the three trips have to be in it).
// - A column that a later version added to `trip` is in the answer of `SELECT *` and is not
//   compared, so a key put into a row of a trip is not a failure (on purpose, tested below).
// - Three reads are used by their value or their numbers and not compared whole, so a key put into
//   them is not a failure: the rows of `meta` (the probe reads `.value`), the count and top of the
//   fixes that "stop a trip" binds into the trip (the trip is compared afterwards), and the count
//   of the other trip's fixes at the end (the probe reads `.n`). Each is tested below.
// - The count that "stop a trip" binds into the trip: a number that comes back as the text of the
//   same number is an integer again in an INTEGER column, so that one alteration is made with a
//   blob, which the column keeps as a blob, and the stopped trip is then found different.

type Row = Record<string, SqlValue>;

/** One answer of the file that has at least a row, in the order the probe reads them. */
interface Read {
  n: number;
  sql: string;
  rows: Row[];
}

/** The statements the first build runs, and the two counts the probe asks for itself. */
const PROBE_STATEMENTS = new Set<string>([
  ...Object.values(Q),
  "SELECT COUNT(*) AS n FROM point",
  "SELECT COUNT(*) AS n FROM batch",
]);

/** A database whose answers go through `change` (the number of the read, its statement, its rows). */
function altered(db: SqlDb, change: (read: Read) => Row[], seen?: (read: Read) => void): SqlDb {
  let n = 0;
  const answer = (sql: string, rows: Row[]): Row[] => {
    if (rows.length === 0 || !PROBE_STATEMENTS.has(sql)) return rows;
    n++;
    const read: Read = { n, sql, rows: rows.map((row) => ({ ...row })) };
    seen?.(read);
    return change(read);
  };
  const wrap = (tx: SqlTx): SqlTx => ({
    ...tx,
    all: async <T>(sql: string, params?: readonly SqlValue[]) =>
      answer(sql, (await tx.all<Row>(sql, params)) as Row[]) as unknown as T[],
    one: async <T>(sql: string, params?: readonly SqlValue[]) => {
      const row = await tx.one<Row>(sql, params);
      return (row === null ? null : (answer(sql, [row])[0] ?? null)) as T | null;
    },
  });
  return { ...db, transaction: (fn) => db.transaction((tx) => fn(wrap(tx))) };
}

/** The reads of a clean run. */
async function reads(db: SqlDb): Promise<Read[]> {
  const out: Read[] = [];
  const result = await probeOlderBuild(
    altered(
      db,
      (read) => read.rows,
      (read) => out.push(read),
    ),
  );
  expect(result).toEqual({ ok: true });
  return out;
}

/** The same value as another kind of value, or the same kind of value that is another value. */
const other = (value: SqlValue): SqlValue =>
  value === null ? "~" : typeof value === "number" ? value + 1000 : `${value}~`;
const asAnotherKind = (value: SqlValue): SqlValue =>
  value === null
    ? 0
    : typeof value === "number"
      ? String(value)
      : (Uint8Array.from(Buffer.from(value)) as unknown as SqlValue);

/** The same value as a blob, which an INTEGER column keeps as a blob (a text of digits it turns back into a number). */
const asBlob = (value: SqlValue): SqlValue =>
  Uint8Array.from(Buffer.from(String(value))) as unknown as SqlValue;

type Alteration = [what: string, change: (row: Row, key: string) => void];
const ALTERATIONS: Alteration[] = [
  ["taken out", (row, key) => void delete row[key]],
  ["given another value", (row, key) => void (row[key] = other(row[key]!))],
  [
    "given the same value as another kind of value",
    (row, key) => void (row[key] = asAnotherKind(row[key]!)),
  ],
];

/** Runs the probe with one key of one row of one read altered. */
async function withAltered(
  db: SqlDb,
  target: { n: number; row: number },
  alter: (row: Row) => void,
) {
  return probeOlderBuild(
    altered(db, (read) => {
      if (read.n !== target.n) return read.rows;
      const rows = read.rows.map((row) => ({ ...row }));
      alter(rows[target.row]!);
      return rows;
    }),
  );
}

// The labels of the nine reads of a trip, in the order of the probe: the start of the message of
// the failure that the alteration of a column has to cause there.
const TRIP_READS = [
  "the trip",
  "the second trip",
  "the stopped trip",
  "the submitted trip",
  "the discarded trip",
  "the patched trip",
  "the problem is cleared",
  "the merged trip",
  "the trip from the server",
];

describe("what the probe reads back, key by key", () => {
  test("a clean run makes the reads these tests go through, and none is a read the probe skips", async () => {
    const all = await reads(await emptyLegacyFile());
    expect(all.filter((read) => read.sql === Q.tripById)).toHaveLength(TRIP_READS.length);
    // Every statement of the probe that answers with rows is among them, but for the ones that
    // answer only with nothing (a trip that is not there) or are not read back.
    const statements = new Set(all.map((read) => read.sql));
    for (const sql of [
      Q.tripById,
      Q.lastPoint,
      Q.countAndTop,
      Q.points,
      Q.pointCount,
      Q.batchPoints,
      Q.openBatch,
      Q.batchTop,
      Q.nextRange,
      Q.selectMeta,
      Q.listTrips,
      "SELECT COUNT(*) AS n FROM point",
      "SELECT COUNT(*) AS n FROM batch",
    ]) {
      expect(statements.has(sql), sql).toBe(true);
    }
    // The number of answers with rows is pinned: a read that the probe loses is noticed here.
    expect(all.length).toBe(28);
  });

  // Every key of every row of every read but the list of trips, taken out, changed in value and
  // changed in kind: the probe fails. One test for each read, so that a failure names it.
  test("every key of every row of every read is compared", async () => {
    const db = await emptyLegacyFile();
    const all = await reads(db);
    let tried = 0;
    const missed: string[] = [];
    // The count of the fixes that "stop a trip" reads is not compared there: it is written into
    // the trip (`expected_points` and `last_seq`, INTEGER columns), and the stopped trip is
    // compared afterwards. A number that comes back as text is an integer again in an INTEGER
    // column, so for that read the same value as another kind of value is a blob, which the column
    // keeps; any other change of the two keys is found at the trip as it is.
    const counted = all.filter((read) => read.sql === Q.countAndTop);
    expect(counted).toHaveLength(2);
    const writtenToTheTrip = counted[1]!.n;
    for (const read of all) {
      if (read.sql === Q.listTrips) continue;
      for (const [row, values] of read.rows.entries()) {
        for (const key of Object.keys(values)) {
          for (const [what, alteration] of ALTERATIONS) {
            const change =
              read.n === writtenToTheTrip && what === ALTERATIONS[2]![0]
                ? (r: Row, k: string) => void (r[k] = asBlob(r[k]!))
                : alteration;
            const result = await withAltered(db, { n: read.n, row }, (r) => change(r, key));
            tried++;
            if (result.ok) {
              missed.push(
                `read ${read.n} (${read.sql.slice(0, 40)}) row ${row} key ${key} ${what}`,
              );
            }
          }
        }
      }
    }
    expect(missed).toEqual([]);
    // 24 columns on 9 trips and the keys of the other answers, three alterations of each, none left out.
    const keys = all
      .filter((read) => read.sql !== Q.listTrips)
      .reduce((n, read) => n + read.rows.reduce((m, row) => m + Object.keys(row).length, 0), 0);
    expect(tried).toBe(keys * ALTERATIONS.length);
    expect(keys).toBeGreaterThanOrEqual(24 * 9 + 40);
  }, 120_000);

  // A key that the statement does not select is held against the answer in both directions: for
  // each statement that the probe compares whole, the first read of it gets a key put in and then
  // loses each key it has, and the probe fails every time. Ten of the thirteen statements are
  // compared whole; the other three are in the test after this one.
  const WHOLE: [name: string, sql: string][] = [
    ["lastPoint", Q.lastPoint],
    ["points", Q.points],
    ["countAndTop", Q.countAndTop],
    ["pointCount", Q.pointCount],
    ["batchPoints", Q.batchPoints],
    ["openBatch", Q.openBatch],
    ["batchTop", Q.batchTop],
    ["nextRange", Q.nextRange],
    ["the count of all the fixes", "SELECT COUNT(*) AS n FROM point"],
    ["the count of all the batches", "SELECT COUNT(*) AS n FROM batch"],
  ];
  test("ten statements are compared whole", () => {
    expect(WHOLE).toHaveLength(10);
  });

  test.each(WHOLE)(
    "the answer of %s is held to the keys it has, in both directions",
    async (_, sql) => {
      const db = await emptyLegacyFile();
      const all = await reads(db);
      const read = all.find((r) => r.sql === sql)!;
      const key = "zz_added";
      const added = await withAltered(db, { n: read.n, row: 0 }, (row) => void (row[key] = 1));
      expect(added.ok, "a key the statement does not select").toBe(false);
      for (const have of Object.keys(read.rows[0]!)) {
        const gone = await withAltered(db, { n: read.n, row: 0 }, (row) => void delete row[have]);
        expect(gone.ok, `without ${have}`).toBe(false);
      }
    },
  );

  // The same for the later reads of these statements: a key put in fails the probe at every read
  // of the ten statements but the two that the next test names (they are used by a value).
  test("a key put in fails the probe at every read of the ten statements but two", async () => {
    const db = await emptyLegacyFile();
    const all = await reads(db);
    const counted = all.filter((r) => r.sql === Q.countAndTop);
    const left = all.filter((r) => r.sql === Q.pointCount);
    const ignored = new Set([counted[1]!.n, left[2]!.n]);
    const whole = new Set(WHOLE.map(([, sql]) => sql));
    let tried = 0;
    for (const read of all.filter((r) => whole.has(r.sql) && !ignored.has(r.n))) {
      const added = await withAltered(db, { n: read.n, row: 0 }, (row) => void (row.zz_added = 1));
      expect(added.ok, `read ${read.n} (${read.sql.slice(0, 40)})`).toBe(false);
      tried++;
    }
    expect(tried).toBe(14);
  });

  // The reads that a key put in does not fail, each for a reason that is written above: the probe
  // takes a value or a number out of them and does not compare the row whole.
  test("a key put into a read that is not compared whole is not a failure", async () => {
    const db = await emptyLegacyFile();
    const all = await reads(db);
    const added = async (read: Read) =>
      withAltered(db, { n: read.n, row: 0 }, (row) => void (row.zz_added = 1));
    // The rows of `meta`: the probe reads `.value`.
    for (const read of all.filter((r) => r.sql === Q.selectMeta)) {
      expect(await added(read), `meta read ${read.n}`).toEqual({ ok: true });
    }
    // The count that "stop a trip" binds into the trip: the second read of the count of the fixes.
    const counted = all.filter((r) => r.sql === Q.countAndTop);
    expect(await added(counted[1]!), "bound into the trip").toEqual({ ok: true });
    expect((await added(counted[0]!)).ok, "the first count is compared whole").toBe(false);
    // The count of the other trip's fixes at the end: the probe reads `.n`. The two before it are whole.
    const left = all.filter((r) => r.sql === Q.pointCount);
    expect(left).toHaveLength(3);
    expect((await added(left[0]!)).ok).toBe(false);
    expect((await added(left[1]!)).ok).toBe(false);
    expect(await added(left[2]!), "read for its n").toEqual({ ok: true });
  });

  test.each(TRIP_READS.map((what, i) => [what, i] as const))(
    "%s: each column of the first layout is named when it is taken out, changed in value or in kind",
    async (what, i) => {
      const db = await emptyLegacyFile();
      const all = (await reads(db)).filter((read) => read.sql === Q.tripById);
      const read = all[i]!;
      expect(Object.keys(read.rows[0]!)).toHaveLength(24);
      for (const key of Object.keys(read.rows[0]!)) {
        for (const [how, change] of ALTERATIONS) {
          const result = await withAltered(db, { n: read.n, row: 0 }, (r) => change(r, key));
          expect(result.ok, `${key} ${how}`).toBe(false);
          const error = (result as { error: string }).error;
          const gone = how === "taken out";
          expect(error, `${key} ${how}`).toMatch(
            new RegExp(`^${what}: the column ${key} is ${gone ? "gone$" : "[^ ]"}`),
          );
        }
      }
    },
  );

  test("a column that a later version added to the trips is in the answer and is not compared", async () => {
    const db = await emptyLegacyFile();
    const all = (await reads(db)).filter((read) => read.sql === Q.tripById);
    for (const read of all) {
      const result = await withAltered(db, { n: read.n, row: 0 }, (row) => {
        row.note = "added later";
        row.flag = 7;
      });
      expect(result, `read ${read.n}`).toEqual({ ok: true });
    }
    // Added to every row of every read of a trip at once.
    const every = await probeOlderBuild(
      altered(db, (read) =>
        read.sql === Q.tripById ? read.rows.map((row) => ({ ...row, note: null })) : read.rows,
      ),
    );
    expect(every).toEqual({ ok: true });
  });

  test("a trip that is not there is not a row of nulls", async () => {
    const db = await emptyLegacyFile();
    const all = (await reads(db)).filter((read) => read.sql === Q.tripById);
    const result = await probeOlderBuild(
      altered(db, (read) => (read.n === all[0]!.n ? [] : read.rows)),
    );
    expect(result).toMatchObject({ ok: false, step: "start a trip" });
    expect((result as { error: string }).error).toBe("the trip: expected a row, got null");
  });

  // The list of trips is the one answer that the probe does not compare whole: the trips of
  // the same minute come back in the order SQLite chooses, and the first build looks a trip up by
  // its id. What it asks is that the three trips are in it.
  test("the list of trips has to hold the three trips, and nothing else of it is compared", async () => {
    const db = await emptyLegacyFile();
    const all = await reads(db);
    const list = all.find((read) => read.sql === Q.listTrips)!;
    expect(list.rows.map((r) => r.client_trip_id)).toHaveLength(3);
    expect(new Set(list.rows.map((r) => r.client_trip_id))).toEqual(
      new Set(["probe-a", "probe-b", "probe-c"]),
    );
    for (const [row, values] of list.rows.entries()) {
      const id = values.client_trip_id as string;
      const without = await withAltered(
        db,
        { n: list.n, row },
        (r) => void delete r.client_trip_id,
      );
      expect(without, id).toMatchObject({ ok: false, step: "list the trips" });
      const renamed = await withAltered(
        db,
        { n: list.n, row },
        (r) => void (r.client_trip_id = "x"),
      );
      expect(renamed, id).toMatchObject({ ok: false, step: "list the trips" });
      const quiet = await withAltered(db, { n: list.n, row }, (r) => void (r.purpose = "other"));
      expect(quiet, "not compared").toEqual({ ok: true });
    }
  });
});

describe("what the probe does not take for a difference, and what it takes for one in its own setup", () => {
  test("the order of the keys of an answer does not matter, the first build reads a row by name", async () => {
    const db = await emptyLegacyFile();
    const backwards = altered(db, (read) =>
      read.rows.map((row) => Object.fromEntries(Object.entries(row).reverse())),
    );
    expect(await probeOlderBuild(backwards)).toEqual({ ok: true });
  });

  afterEach(() => {
    vi.doUnmock("../test-support/legacy-statements.ts");
    vi.resetModules();
  });

  // The probe gives `patchTrip` one value for each column the statement sets, and says so when the
  // statement of the first build and the values of the probe are not the same list: here the
  // statement is made to set one column more than the probe gives a value for.
  test("a patchTrip that sets a column the probe has no value for is said so, before it runs", async () => {
    vi.resetModules();
    vi.doMock("../test-support/legacy-statements.ts", async (original) => {
      const real = await original<typeof import("../test-support/legacy-statements.ts")>();
      return {
        ...real,
        FIRST_BUILD_SQL: {
          ...real.FIRST_BUILD_SQL,
          patchTrip: real.FIRST_BUILD_SQL.patchTrip.replace(
            " WHERE client_trip_id = ?",
            ", created_ms = ? WHERE client_trip_id = ?",
          ),
        },
      };
    });
    const probe = await import("../test-support/older-build-probe.ts");
    expect(await probe.probeOlderBuild(await emptyLegacyFile())).toEqual({
      ok: false,
      step: "patch a trip with every column it can set, then clear its problem",
      error: "the values of the probe are not the columns that patchTrip sets",
    });
  });
});

// The comparison on its own, with a made-up row of three columns.
describe("the comparison of a row with what the first build expects", () => {
  const columns = ["id", "name", "n"];
  const row: Row = { id: "a", name: null, n: 0 };
  const same = (answer: Row | null, expected: Row) =>
    expectRow("the row", columns, answer, expected);

  test("passes when every column has the value and the kind of value expected", () => {
    expect(() => same(row, { id: "a", name: null, n: 0 })).not.toThrow();
    // The order of the keys does not matter.
    expect(() => same(row, { n: 0, name: null, id: "a" })).not.toThrow();
  });

  test("does not look at a column that is not one of the first layout's", () => {
    expect(() => same({ ...row, added: "later" }, { id: "a", name: null, n: 0 })).not.toThrow();
  });

  test.each<[string, Row | null, Row, string]>([
    [
      "a row that is not there",
      null,
      { id: "a", name: null, n: 0 },
      "the row: expected a row, got null",
    ],
    [
      "a column that is gone",
      { id: "a", n: 0 },
      { id: "a", name: null, n: 0 },
      "the row: the column name is gone",
    ],
    [
      "another text",
      { ...row, id: "b" },
      { id: "a", name: null, n: 0 },
      'the row: the column id is "b", expected "a"',
    ],
    [
      "a number for NULL",
      { ...row, name: 0 },
      { id: "a", name: null, n: 0 },
      "the row: the column name is 0, expected null",
    ],
    [
      "NULL for a number",
      { ...row, n: null },
      { id: "a", name: null, n: 0 },
      "the row: the column n is null, expected 0",
    ],
    [
      "text for a number",
      { ...row, n: "0" },
      { id: "a", name: null, n: 0 },
      'the row: the column n is "0", expected 0',
    ],
    [
      "a number for text",
      { ...row, id: 1 },
      { id: "1", name: null, n: 0 },
      'the row: the column id is 1, expected "1"',
    ],
    [
      "a value that was expected to be NULL",
      { ...row, name: "x" },
      { id: "a", name: null, n: 0 },
      'the row: the column name is "x", expected null',
    ],
    [
      "a column that the probe does not expect a value for",
      row,
      { id: "a", name: null },
      "the row: the probe expects no value for the column n",
    ],
    [
      "a column that the probe expects and the first layout lacks",
      row,
      { id: "a", name: null, n: 0, extra: 1 },
      "the row: the probe expects a column extra, which the first layout lacks",
    ],
  ])("fails for %s", (_, answer, expected, message) => {
    expect(() => same(answer, expected)).toThrow(new Error(message));
  });

  // JSON writes Infinity, -Infinity and NaN as null and -0 as 0, so a comparison through plain JSON
  // takes them for NULL and for 0. SQLite can answer Infinity (`SELECT 9e999`). They are tagged.
  test.each<[string, SqlValue, SqlValue]>([
    ["Infinity for NULL", Infinity, null],
    ["-Infinity for NULL", -Infinity, null],
    ["NaN for NULL", NaN, null],
    ["NULL for Infinity", null, Infinity],
    ["NULL for NaN", null, NaN],
    ["Infinity for -Infinity", Infinity, -Infinity],
    ["Infinity for NaN", Infinity, NaN],
    ["-Infinity for NaN", -Infinity, NaN],
    ["-0 for 0", -0, 0],
    ["0 for -0", 0, -0],
    ["-0 for NULL", -0, null],
    ["Infinity for its text", Infinity, "Infinity"],
    ["NaN for its text", NaN, "NaN"],
    ["-0 for its text", -0, "-0"],
  ])("tells %s", (_, got, want) => {
    expect(() => same({ ...row, n: got }, { id: "a", name: null, n: want })).toThrow(
      /the row: the column n is /,
    );
  });

  test("says what it found, with the tag", () => {
    expect(() => same({ ...row, n: -0 }, { id: "a", name: null, n: 0 })).toThrow(
      new Error('the row: the column n is {"$number":"-0"}, expected 0'),
    );
    expect(() => same({ ...row, n: Infinity }, { id: "a", name: null, n: 0 })).toThrow(
      new Error('the row: the column n is {"$number":"Infinity"}, expected 0'),
    );
    expect(() => same({ ...row, n: NaN }, { id: "a", name: null, n: 0 })).toThrow(
      new Error('the row: the column n is {"$number":"NaN"}, expected 0'),
    );
  });

  test.each([Infinity, -Infinity, NaN, -0])("passes %s when it is what is expected", (value) => {
    expect(() => same({ ...row, n: value }, { id: "a", name: null, n: value })).not.toThrow();
  });

  test("a BigInt is not a number and is not an error of the comparison", () => {
    const big = 1n as unknown as SqlValue;
    expect(() => same({ ...row, n: big }, { id: "a", name: null, n: 1 })).toThrow(
      /the row: the column n is /,
    );
    expect(() => same({ ...row, n: big }, { id: "a", name: null, n: big })).not.toThrow();
  });

  test("tells a blob from a text of the same bytes", () => {
    const blob = Uint8Array.from(Buffer.from("a")) as unknown as SqlValue;
    expect(() => same({ ...row, id: blob }, { id: "a", name: null, n: 0 })).toThrow(
      /the column id is /,
    );
  });
});

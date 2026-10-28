/**
 * The small SQL surface the outbox needs. The phone implements it over `expo-sqlite` (encrypted
 * with SQLCipher); the tests implement it over `node:sqlite`. Both go through `createSqlDb`, which
 * adds the one thing SQLite connections do not give: a transaction that no other caller's query
 * can run inside.
 */

export type SqlValue = string | number | null;

/** What a driver offers. Nothing here is serialised: `createSqlDb` does that. */
export interface RawDb {
  exec(sql: string): Promise<void>;
  run(sql: string, params: readonly SqlValue[]): Promise<void>;
  all(sql: string, params: readonly SqlValue[]): Promise<Record<string, SqlValue>[]>;
}

export interface SqlDb {
  exec(sql: string): Promise<void>;
  run(sql: string, params?: readonly SqlValue[]): Promise<void>;
  all<T = Record<string, SqlValue>>(sql: string, params?: readonly SqlValue[]): Promise<T[]>;
  one<T = Record<string, SqlValue>>(sql: string, params?: readonly SqlValue[]): Promise<T | null>;
  /**
   * Runs `fn` in one write transaction (`BEGIN IMMEDIATE`) and commits it when `fn` resolves; if
   * `fn` rejects, everything is rolled back and the error is thrown again. Calls made by other
   * code wait until the transaction ends. Inside `fn`, use only the `tx` it is given.
   */
  transaction<T>(fn: (tx: SqlTx) => Promise<T>): Promise<T>;
}

/** The queries of a running transaction (they do not wait for the transaction they belong to). */
export type SqlTx = Omit<SqlDb, "transaction">;

function surface(raw: RawDb): SqlTx {
  return {
    exec: (sql) => raw.exec(sql),
    run: (sql, params = []) => raw.run(sql, params),
    all: async <T>(sql: string, params: readonly SqlValue[] = []) =>
      (await raw.all(sql, params)) as unknown as T[],
    one: async <T>(sql: string, params: readonly SqlValue[] = []) =>
      ((await raw.all(sql, params))[0] as unknown as T | undefined) ?? null,
  };
}

export function createSqlDb(raw: RawDb): SqlDb {
  const inner = surface(raw);
  // A queue of promises: each call or transaction starts when the one before it has ended.
  let tail: Promise<unknown> = Promise.resolve();
  const queued = <T>(job: () => Promise<T>): Promise<T> => {
    const result = tail.then(job, job);
    tail = result.catch(() => undefined);
    return result;
  };
  return {
    exec: (sql) => queued(() => inner.exec(sql)),
    run: (sql, params) => queued(() => inner.run(sql, params)),
    all: (sql, params) => queued(() => inner.all(sql, params)),
    one: (sql, params) => queued(() => inner.one(sql, params)),
    transaction: <T>(fn: (tx: SqlTx) => Promise<T>) =>
      queued(async () => {
        await raw.exec("BEGIN IMMEDIATE");
        try {
          const value = await fn(inner);
          await raw.exec("COMMIT");
          return value;
        } catch (error) {
          try {
            await raw.exec("ROLLBACK");
          } catch {
            // The original error is the one to report.
          }
          throw error;
        }
      }),
  };
}

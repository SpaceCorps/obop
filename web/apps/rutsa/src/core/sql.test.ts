import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import type { SqlDb } from "./sql.ts";

let db: SqlDb;

beforeEach(async () => {
  db = memoryDb();
  await db.exec("CREATE TABLE item (id INTEGER PRIMARY KEY, name TEXT NOT NULL)");
});

const count = async (): Promise<number> =>
  (await db.one<{ n: number }>("SELECT COUNT(*) AS n FROM item"))?.n ?? -1;

describe("queries", () => {
  test("a row that is written can be read, with its parameters", async () => {
    await db.run("INSERT INTO item (name) VALUES (?)", ["first"]);
    await db.run("INSERT INTO item (name) VALUES (?)", ["second"]);
    const rows = await db.all<{ name: string }>("SELECT name FROM item ORDER BY id");
    expect(rows.map((r) => r.name)).toEqual(["first", "second"]);
  });

  test("one gives the first row, or null when there is none", async () => {
    expect(await db.one("SELECT name FROM item")).toBeNull();
    await db.run("INSERT INTO item (name) VALUES (?)", ["only"]);
    expect(await db.one<{ name: string }>("SELECT name FROM item")).toEqual({ name: "only" });
  });

  test("a failed query does not stop the queue", async () => {
    await expect(db.run("INSERT INTO nothing (name) VALUES ('x')")).rejects.toThrow();
    await db.run("INSERT INTO item (name) VALUES (?)", ["after"]);
    expect(await count()).toBe(1);
  });
});

describe("transactions", () => {
  test("everything is kept when the function resolves, and its value is returned", async () => {
    const value = await db.transaction(async (tx) => {
      await tx.run("INSERT INTO item (name) VALUES (?)", ["a"]);
      await tx.run("INSERT INTO item (name) VALUES (?)", ["b"]);
      return "done";
    });
    expect(value).toBe("done");
    expect(await count()).toBe(2);
  });

  test("nothing is kept when the function fails, and its error is thrown again", async () => {
    await db.run("INSERT INTO item (name) VALUES (?)", ["before"]);
    await expect(
      db.transaction(async (tx) => {
        await tx.run("INSERT INTO item (name) VALUES (?)", ["half"]);
        throw new Error("stop halfway");
      }),
    ).rejects.toThrow("stop halfway");
    const rows = await db.all<{ name: string }>("SELECT name FROM item");
    expect(rows.map((r) => r.name)).toEqual(["before"]);
  });

  test("a query made during a transaction waits for its end and sees what it committed", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const order: string[] = [];
    const tx = db.transaction(async (t) => {
      await t.run("INSERT INTO item (name) VALUES (?)", ["inside"]);
      await gate;
      order.push("commit");
    });
    const outside = count().then((n) => {
      order.push("outside");
      return n;
    });
    // Let the outside query try to run while the transaction is still open.
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(order).toEqual([]);
    release();
    await tx;
    expect(await outside).toBe(1);
    expect(order).toEqual(["commit", "outside"]);
  });

  test("a transaction after a failed one starts clean", async () => {
    await expect(
      db.transaction(async () => {
        throw new Error("first fails");
      }),
    ).rejects.toThrow("first fails");
    await db.transaction(async (tx) => {
      await tx.run("INSERT INTO item (name) VALUES (?)", ["second"]);
    });
    expect(await count()).toBe(1);
  });
});

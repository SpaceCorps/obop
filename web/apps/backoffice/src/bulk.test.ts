import { describe, expect, it } from "vite-plus/test";
import type { Row } from "./api.ts";
import { BULK_CAP, collect, inBatches, outcomeOf, tally, type Outcome } from "./bulk.ts";
import { NETWORK_MESSAGE } from "./net.ts";

const items = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `r${i}`, label: `R${i}` }));
const done = (item: { id: string; label: string }): Outcome => ({ ...item, status: "done" });
const rows = (from: number, n: number): Row[] =>
  Array.from({ length: n }, (_, i) => ({ id: `r${from + i}` }));

describe("inBatches", () => {
  it("applies the work in order, a batch at a time, and reports progress", async () => {
    let running = 0;
    let peak = 0;
    const order: string[] = [];
    const progress: number[] = [];
    const out = await inBatches(
      items(25),
      async (item) => {
        running++;
        peak = Math.max(peak, running);
        await new Promise((r) => setTimeout(r, 1));
        order.push(item.id);
        running--;
        return done(item);
      },
      { batch: 8, onProgress: (n) => progress.push(n) },
    );
    expect(out.results).toHaveLength(25);
    expect(out.cancelled).toBe(false);
    expect(peak).toBeLessThanOrEqual(8);
    expect(progress).toEqual([8, 16, 24, 25]);
    // Batches start in order even though members of a batch may finish in any order.
    expect(order.slice(0, 8).sort()).toEqual(
      items(8)
        .map((i) => i.id)
        .sort(),
    );
  });

  it("stops between batches when cancelled and leaves the rest untouched", async () => {
    let stop = false;
    const out = await inBatches(
      items(30),
      async (item) => {
        if (item.id === "r9") stop = true;
        return done(item);
      },
      { batch: 5, cancelled: () => stop },
    );
    expect(out.cancelled).toBe(true);
    // r9 sits in the second batch (r5..r9), which finishes; the third never starts.
    expect(out.results).toHaveLength(10);
  });

  it("stops when a whole batch could not reach the server", async () => {
    const out = await inBatches(
      items(40),
      async (item) => ({ ...item, status: "failed", message: NETWORK_MESSAGE }),
      { batch: 4 },
    );
    expect(out.stopped).toBe(true);
    expect(out.results).toHaveLength(4);
  });

  it("keeps going when only some records fail, and turns a thrown error into a failure", async () => {
    const out = await inBatches(
      items(6),
      async (item) => {
        if (item.id === "r1") throw new Error("boom");
        if (item.id === "r2") return { ...item, status: "failed", message: "no" } as Outcome;
        return done(item);
      },
      { batch: 3 },
    );
    expect(out.stopped).toBe(false);
    expect(tally(out.results)).toEqual({
      done: 4,
      approval: 0,
      referenced: 0,
      failed: 2,
      skipped: 0,
    });
  });
});

describe("collect", () => {
  const server = (total: number) => async (offset: number, limit: number) => ({
    rows: rows(offset, Math.max(0, Math.min(limit, total - offset))),
    total,
  });

  it("pages through all matches", async () => {
    const calls: [number, number][] = [];
    const got = await collect(
      async (o, l) => {
        calls.push([o, l]);
        return server(250)(o, l);
      },
      BULK_CAP,
      { pageSize: 100 },
    );
    if ("error" in got) throw new Error(got.error);
    expect(got.rows).toHaveLength(250);
    expect(got.truncated).toBe(false);
    expect(calls).toEqual([
      [0, 100],
      [100, 100],
      [200, 100],
    ]);
    expect(new Set(got.rows.map((r) => r.id)).size).toBe(250);
  });

  it("stops at the cap and says there were more", async () => {
    const got = await collect(server(12_000), BULK_CAP, { pageSize: 1000 });
    if ("error" in got) throw new Error(got.error);
    expect(got.rows).toHaveLength(BULK_CAP);
    expect(got.total).toBe(12_000);
    expect(got.truncated).toBe(true);
  });

  it("returns the server's error, and a cancel", async () => {
    expect(await collect(async () => ({ error: "unknown field lead.x" }), 10)).toEqual({
      error: "unknown field lead.x",
    });
    let stop = false;
    const got = await collect(
      async (o, l) => {
        stop = true;
        return server(500)(o, l);
      },
      BULK_CAP,
      { pageSize: 100, cancelled: () => stop },
    );
    if ("error" in got) throw new Error(got.error);
    expect(got.cancelled).toBe(true);
    expect(got.rows).toHaveLength(100);
  });

  it("does not loop on an empty page", async () => {
    const got = await collect(async () => ({ rows: [], total: 9 }), BULK_CAP);
    if ("error" in got) throw new Error(got.error);
    expect(got.rows).toHaveLength(0);
  });
});

describe("outcomeOf", () => {
  const row = { id: "a", label: "A" };
  it("maps each kind of reply", () => {
    expect(outcomeOf(row, { kind: "ok", value: {}, denied: [] }).status).toBe("done");
    expect(outcomeOf(row, { kind: "approval", planId: "p1" })).toMatchObject({
      status: "approval",
      planId: "p1",
    });
    expect(
      outcomeOf(row, { kind: "error", status: 400, message: "lead.stage is required" }).status,
    ).toBe("failed");
    expect(
      outcomeOf(row, {
        kind: "error",
        status: 400,
        message: "cannot delete party P: still referenced by lead (1): x",
      }).status,
    ).toBe("referenced");
  });
});

import { ObopNetworkError } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import { DeadlineError, withDeadline } from "./deadline.ts";

describe("a call with a deadline", () => {
  test("gives the answer of a call that comes in time", async () => {
    expect(await withDeadline(1000, async () => 7)).toBe(7);
  });

  test("aborts the signal of a call that has no answer in time, and the call ends", async () => {
    let seen: AbortSignal | null = null;
    const stalled = withDeadline(
      20,
      (signal) =>
        new Promise<never>((_resolve, reject) => {
          seen = signal;
          signal.addEventListener("abort", () => reject(new Error("aborted")));
        }),
    );
    await expect(stalled).rejects.toThrow("aborted");
    expect((seen as AbortSignal | null)?.aborted).toBe(true);
  });

  test("does not abort a call that has finished", async () => {
    let seen: AbortSignal | null = null;
    await withDeadline(20, async (signal) => {
      seen = signal;
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect((seen as AbortSignal | null)?.aborted).toBe(false);
  });

  test("ends a call that ignores the signal, with a network error whose cause is the deadline", async () => {
    // A fetch that never notices the abort: only the race can end this call.
    let seen: AbortSignal | null = null;
    const started = Date.now();
    const stuck = withDeadline(
      30,
      (signal) =>
        new Promise<never>(() => {
          seen = signal;
        }),
    );
    const error = await stuck.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ObopNetworkError);
    expect((error as ObopNetworkError).cause).toBeInstanceOf(DeadlineError);
    expect(Date.now() - started).toBeLessThan(2000);
    expect((seen as AbortSignal | null)?.aborted).toBe(true);
  });

  test("leaves nothing unhandled when the call ends before the deadline", async () => {
    const seen: unknown[] = [];
    const listener = (reason: unknown) => seen.push(reason);
    process.on("unhandledRejection", listener);
    try {
      expect(await withDeadline(20, async () => 1)).toBe(1);
      await new Promise((resolve) => setTimeout(resolve, 60));
    } finally {
      process.off("unhandledRejection", listener);
    }
    expect(seen).toEqual([]);
  });
});

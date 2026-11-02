import { ObopNetworkError } from "@obop/client";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { DeadlineError, withDeadline } from "./deadline.ts";

describe("a call with a deadline", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  test("gives the answer of a call that comes in time", async () => {
    expect(await withDeadline(1000, async () => 7)).toBe(7);
  });

  test("aborts the signal of a call that has no answer in time, and the call ends with a network error", async () => {
    // The call rejects at once when it is aborted, before any other step: its own error must not
    // win over the deadline's.
    let seen: AbortSignal | null = null;
    const stalled = withDeadline(
      20,
      (signal) =>
        new Promise<never>((_resolve, reject) => {
          seen = signal;
          signal.addEventListener("abort", () => reject(new Error("aborted")));
        }),
    );
    const error = await stalled.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ObopNetworkError);
    expect((error as ObopNetworkError).cause).toBeInstanceOf(DeadlineError);
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

  test("leaves no timer running when the call ends, in an answer or in an error", async () => {
    vi.useFakeTimers();
    expect(await withDeadline(1000, async () => 1)).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
    await expect(
      withDeadline(1000, async () => {
        throw new Error("refused");
      }),
    ).rejects.toThrow("refused");
    expect(vi.getTimerCount()).toBe(0);
  });

  test("leaves nothing unhandled when a call that was given up ends later", async () => {
    // The loser of the race: the deadline came first, the call settles afterwards. This fails when
    // the late end of a call that was given up has no handler (the late rejection is then reported
    // as unhandled) and when the race is removed.
    const seen: unknown[] = [];
    const listener = (reason: unknown) => seen.push(reason);
    process.on("unhandledRejection", listener);
    try {
      let later: (error: Error) => void = () => undefined;
      const given = withDeadline(
        20,
        () =>
          new Promise<never>((_resolve, reject) => {
            later = reject;
          }),
      );
      await expect(given).rejects.toBeInstanceOf(ObopNetworkError);
      later(new Error("late"));
      await new Promise((resolve) => setTimeout(resolve, 60));
    } finally {
      process.off("unhandledRejection", listener);
    }
    expect(seen).toEqual([]);
  });
});

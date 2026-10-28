import { describe, expect, test } from "vite-plus/test";
import { withDeadline } from "./deadline.ts";

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
});

import { describe, expect, test } from "vite-plus/test";
import { createSerial } from "./serial.ts";

/** A promise the test resolves by hand. */
function gate() {
  let open = () => {};
  const done = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { done, open };
}

describe("jobs one after another", () => {
  test("a job starts only when the one before it has ended", async () => {
    const serial = createSerial();
    const log: string[] = [];
    const first = gate();
    const a = serial(async () => {
      log.push("a starts");
      await first.done;
      log.push("a ends");
    });
    const b = serial(async () => {
      log.push("b starts");
    });
    await Promise.resolve();
    expect(log).toEqual(["a starts"]);
    first.open();
    await Promise.all([a, b]);
    expect(log).toEqual(["a starts", "a ends", "b starts"]);
  });

  test("a job that fails gives its error to its caller and does not stop the next", async () => {
    const serial = createSerial();
    await expect(serial(async () => Promise.reject(new Error("refused")))).rejects.toThrow(
      "refused",
    );
    await expect(serial(async () => "next")).resolves.toBe("next");
  });

  test("the check of a recording and a Start on the same return do not interleave", async () => {
    const serial = createSerial();
    let running = false;
    const log: string[] = [];
    // The check finds no trip and stops the updates; the Start then stores the trip and starts them.
    const check = serial(async () => {
      log.push("check reads: no trip");
      await Promise.resolve();
      log.push(`check stops the updates (running: ${running})`);
      running = false;
    });
    const start = serial(async () => {
      log.push("start stores the trip");
      await Promise.resolve();
      running = true;
      log.push("start starts the updates");
    });
    await Promise.all([check, start]);
    expect(log).toEqual([
      "check reads: no trip",
      "check stops the updates (running: false)",
      "start stores the trip",
      "start starts the updates",
    ]);
    expect(running).toBe(true);
  });
});

import { ObopNetworkError } from "@obop/client";

/** Why a call was given up: the cause of the `ObopNetworkError` that `withDeadline` throws. */
export class DeadlineError extends Error {
  constructor(readonly ms: number) {
    super(`No answer within ${ms} ms`);
    this.name = "DeadlineError";
  }
}

/**
 * Runs a call that is given up when it has no answer after `ms` milliseconds. Two things happen
 * then: the result is a rejection with an `ObopNetworkError` whose cause is a `DeadlineError`
 * (the rejection is made first, so it is the result even for a call that rejects at once when it
 * is aborted), and the `signal` the call was handed is aborted (the call is expected to pass it to
 * `fetch`, so that the request itself stops). The race matters because React Native's network
 * stack has no timeout of its own and a call that does not honour the signal (or a half-open
 * connection that keeps `fetch` pending after the abort) would otherwise stall for minutes. The
 * timer is cleared as soon as the call has ended, in an answer or in an error.
 */
export async function withDeadline<T>(
  ms: number,
  run: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stalled = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      // The rejection first: a call that rejects in its abort listener must not win with its own
      // error. The race holds a handler on `stalled` from the start, so nothing is left unhandled.
      reject(new ObopNetworkError(new DeadlineError(ms)));
      controller.abort();
    }, ms);
  });
  try {
    return await Promise.race([run(controller.signal), stalled]);
  } finally {
    clearTimeout(timer);
  }
}

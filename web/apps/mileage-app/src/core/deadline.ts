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
 * then: the `signal` it is handed is aborted (the call is expected to pass it to `fetch`, so that
 * the request itself stops), and the result is a rejection with an `ObopNetworkError` whether or
 * not the call has noticed the signal. The race matters because React Native's network stack has
 * no timeout of its own and a call that does not honour the signal (or a half-open connection that
 * keeps `fetch` pending after the abort) would otherwise stall for minutes. The timer is always
 * cleared.
 */
export async function withDeadline<T>(
  ms: number,
  run: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stalled = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      controller.abort();
      reject(new ObopNetworkError(new DeadlineError(ms)));
    }, ms);
  });
  // The call may end first and leave the race lost: the rejection must not go unhandled.
  stalled.catch(() => undefined);
  try {
    return await Promise.race([run(controller.signal), stalled]);
  } finally {
    clearTimeout(timer);
  }
}

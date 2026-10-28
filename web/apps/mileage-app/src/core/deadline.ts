/**
 * Runs a call that is given up when it has no answer after `ms` milliseconds: the `signal` it is
 * handed is aborted then, and the call is expected to pass it to `fetch` (React Native's network
 * stack has no timeout of its own, so without this a half-open connection stalls for minutes).
 * The timer is always cleared.
 */
export async function withDeadline<T>(
  ms: number,
  run: (signal: AbortSignal) => Promise<T>,
): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ms);
  try {
    return await run(controller.signal);
  } finally {
    clearTimeout(timer);
  }
}

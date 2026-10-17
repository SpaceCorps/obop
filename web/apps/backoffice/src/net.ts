import { useSyncExternalStore } from "react";

/**
 * Network state and the one fetch the app uses for its API.
 *
 * - `request()` never lets a dropped connection escape as an unhandled rejection: it throws a
 *   `NetworkError` that callers turn into a normal error reply.
 * - Idempotent reads (get, list, search, meta) retry twice with a short backoff when the connection
 *   drops or a gateway answers 502, 503 or 504. Writes are never retried: they might have landed.
 * - The page-wide offline banner reads `useNet()`: it is shown while the browser says it is offline
 *   or the last request could not reach the server, and clears at the next answer.
 */
export class NetworkError extends Error {
  constructor() {
    super("Could not reach the server");
    this.name = "NetworkError";
  }
}

/** English texts of the two transport failures; `serverMessage()` translates them. */
export const NETWORK_MESSAGE = "Could not reach the server";
export const serverFault = (status: number) => `The server had a problem (status ${status})`;

type NetState = { offline: boolean; unreachable: boolean };
let state: NetState = {
  offline: typeof navigator !== "undefined" && navigator.onLine === false,
  unreachable: false,
};
const listeners = new Set<() => void>();
const set = (patch: Partial<NetState>) => {
  const next = { ...state, ...patch };
  if (next.offline === state.offline && next.unreachable === state.unreachable) return;
  state = next;
  for (const l of listeners) l();
};
const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};

if (typeof window !== "undefined") {
  window.addEventListener("offline", () => set({ offline: true }));
  window.addEventListener("online", () => set({ offline: false, unreachable: false }));
}

/** Whether the page is cut off from the server right now. */
export function useNet(): { down: boolean; offline: boolean } {
  const s = useSyncExternalStore(subscribe, () => state);
  return { down: s.offline || s.unreachable, offline: s.offline };
}

/** Delays between attempts of an idempotent read. Tests replace it to run fast. */
export const backoff = { delays: [250, 750] };

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const GATEWAY = new Set([502, 503, 504]);

export async function request(
  input: string,
  init: RequestInit,
  opts: { retry?: boolean } = {},
): Promise<Response> {
  const delays = opts.retry ? backoff.delays : [];
  for (let attempt = 0; ; attempt++) {
    try {
      const res = await fetch(input, init);
      if (GATEWAY.has(res.status) && attempt < delays.length) {
        await wait(delays[attempt]!);
        continue;
      }
      set({ unreachable: false });
      return res;
    } catch {
      if (attempt < delays.length) {
        await wait(delays[attempt]!);
        continue;
      }
      set({ unreachable: true });
      throw new NetworkError();
    }
  }
}

/** Ask the server once whether it answers; clears the banner when it does. */
export async function probe(): Promise<boolean> {
  try {
    await fetch("/", { method: "HEAD", cache: "no-store" });
    set({ unreachable: false, offline: typeof navigator !== "undefined" && !navigator.onLine });
    return true;
  } catch {
    set({ unreachable: true });
    return false;
  }
}

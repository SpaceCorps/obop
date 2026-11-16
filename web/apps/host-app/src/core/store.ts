// The reducer with its state held outside React, so that a WebView's question ("may this
// address load?") is answered from the state as it is at that instant, not as it was at the last
// render. Events are applied one after the other, in the order they are sent.
import type { Build } from "./build.ts";
import { initialState, reduce, type ShellEvent, type ShellState } from "./shell.ts";

export interface ShellStore {
  /** The state now. */
  readonly get: () => ShellState;
  readonly dispatch: (event: ShellEvent) => void;
  /** `listener` is called after every event that changed the state. Returns the function that stops it. */
  readonly subscribe: (listener: () => void) => () => void;
}

export function createShellStore(build: Build): ShellStore {
  let state = initialState(build);
  const listeners = new Set<() => void>();
  return {
    get: () => state,
    dispatch: (event) => {
      const next = reduce(state, event);
      if (next === state) return;
      state = next;
      for (const listener of listeners) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

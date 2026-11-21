import { describe, expect, test } from "vite-plus/test";
import { onForeground, type AppStateLike } from "./foreground.ts";

/** An `AppState` the test moves by hand. */
function fakeAppState() {
  const listeners = new Set<(state: string) => void>();
  const appState: AppStateLike = {
    addEventListener: (_type, listener) => {
      listeners.add(listener);
      return { remove: () => listeners.delete(listener) };
    },
  };
  return { appState, go: (state: string) => listeners.forEach((l) => l(state)), listeners };
}

describe("running something now and on every return to the front", () => {
  test("runs at once, and again each time the app becomes active", () => {
    const { appState, go } = fakeAppState();
    let runs = 0;
    onForeground(appState, () => {
      runs += 1;
    });
    expect(runs).toBe(1);
    go("background");
    expect(runs).toBe(1);
    go("active");
    expect(runs).toBe(2);
    go("inactive");
    go("active");
    expect(runs).toBe(3);
  });

  test("stops listening when asked", () => {
    const { appState, go, listeners } = fakeAppState();
    let runs = 0;
    const stop = onForeground(appState, () => {
      runs += 1;
    });
    stop();
    go("active");
    expect(runs).toBe(1);
    expect(listeners.size).toBe(0);
  });
});

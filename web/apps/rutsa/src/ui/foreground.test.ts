import { describe, expect, test } from "vite-plus/test";
import { onForeground, onReturn, onReturnFromAway, type AppStateLike } from "./foreground.ts";

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

  test("on a return only: nothing now, then each time the app becomes active", () => {
    const { appState, go, listeners } = fakeAppState();
    let runs = 0;
    const stop = onReturn(appState, () => {
      runs += 1;
    });
    expect(runs).toBe(0);
    go("active");
    expect(runs).toBe(1);
    stop();
    go("active");
    expect(runs).toBe(1);
    expect(listeners.size).toBe(0);
  });

  test("back from away: the background, then active, runs it once", () => {
    const { appState, go } = fakeAppState();
    let runs = 0;
    onReturnFromAway(
      appState,
      () => false,
      () => {
        runs += 1;
      },
    );
    go("active");
    expect(runs).toBe(0);
    go("background");
    go("active");
    expect(runs).toBe(1);
    go("active");
    expect(runs).toBe(1);
  });

  test("a trip to the background while the app's own dialog is open does not count", () => {
    const { appState, go } = fakeAppState();
    let asking = true;
    let runs = 0;
    onReturnFromAway(
      appState,
      () => asking,
      () => {
        runs += 1;
      },
    );
    // Android: the permission dialog sends the app to the background, its answer brings it back.
    go("background");
    asking = false;
    go("active");
    expect(runs).toBe(0);
    // Then the driver goes to the settings and comes back.
    go("background");
    go("active");
    expect(runs).toBe(1);
  });

  test("inactive (an iOS dialog, the app switcher) is not away", () => {
    const { appState, go, listeners } = fakeAppState();
    let runs = 0;
    const stop = onReturnFromAway(
      appState,
      () => false,
      () => {
        runs += 1;
      },
    );
    go("inactive");
    go("active");
    expect(runs).toBe(0);
    stop();
    expect(listeners.size).toBe(0);
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

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, test, vi } from "vite-plus/test";
import { refreshOnShow, type AppStateLike } from "./foreground.ts";

function fakeAppState() {
  const listeners: Array<(state: string) => void> = [];
  const removed = vi.fn();
  const appState: AppStateLike = {
    addEventListener: (_type, listener) => {
      listeners.push(listener);
      return { remove: removed };
    },
  };
  return { appState, emit: (s: string) => listeners.forEach((l) => l(s)), removed };
}

describe("refreshing what a screen shows when it opens and when the app comes back (L27)", () => {
  test("once at once, again on each return to the foreground, never in the background", () => {
    const { appState, emit, removed } = fakeAppState();
    const refresh = vi.fn();
    const stop = refreshOnShow(appState, refresh);
    expect(refresh).toHaveBeenCalledTimes(1);
    emit("background");
    emit("inactive");
    expect(refresh).toHaveBeenCalledTimes(1);
    emit("active");
    expect(refresh).toHaveBeenCalledTimes(2);
    stop();
    expect(removed).toHaveBeenCalledTimes(1);
  });

  test("Home uses it to read the driver's trips from the server", () => {
    const home = readFileSync(
      fileURLToPath(new URL("./screens/Home.tsx", import.meta.url)),
      "utf8",
    );
    expect(home).toMatch(/refreshOnShow\(\s*AppState,/);
    expect(home).toContain("rt.engine.refreshRemote()");
  });
});

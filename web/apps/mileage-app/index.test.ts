import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { StartupConfig } from "./src/core/startup-config.ts";

// The entry of the app (`index.ts`) is the one place that gives the host its configuration. If the
// call is missing, or is made with another object, or is made after the app has been handed to
// the system, nothing else notices until a screen or the background location task asks for the
// configuration and finds none: the task catches that and logs it, so fixes would be dropped
// without a sign. So the entry is run here, with the parts that need a phone replaced:
//
// - the task module and the root component are stand-ins that note when they are loaded;
// - `expo`'s `registerRootComponent` notes that it was called, with what, and what the host's
//   configuration was at that moment;
// - the host (`startHost`, `hostConfig`) and the startup configuration are the real ones.
//
// Not pinned by any test, and said here so that nobody takes the tests below for more:
// - that the system calls the location task only after the entry has run to its end (read from the
//   source of expo-task-manager and react-native, not run on a phone);
// - that Metro runs the imports of the entry in the order written (here they run in Vitest's
//   order, which is the order written; Metro compiles them to `require` calls in that order);
// - what the task module does (`task-closure.test.ts` and `host.test.ts` cover its imports and
//   the laziness of the readers; here it is a stand-in).

const seen = vi.hoisted(() => ({
  /** The modules of the app that the entry loaded, in the order they were loaded, and "register". */
  events: [] as string[],
  registered: [] as unknown[],
  /** What `hostConfig()` answered when `registerRootComponent` was called; null if it threw. */
  hostAtRegister: undefined as StartupConfig | null | undefined,
}));

// `vi.doMock` in `beforeEach`, not `vi.mock` at the top: the result of a `vi.mock` factory is kept
// across `vi.resetModules()`, and the stand-in for `expo` has to see the host of the same run.
beforeEach(() => {
  vi.resetModules();
  seen.events.length = 0;
  seen.registered.length = 0;
  seen.hostAtRegister = undefined;

  vi.doMock("./src/native/task.ts", () => {
    seen.events.push("task");
    return {};
  });
  vi.doMock("./App.tsx", () => {
    seen.events.push("App");
    return { default: function App() {} };
  });
  vi.doMock("expo", async () => {
    seen.events.push("expo");
    const host = await import("./src/native/host.ts");
    return {
      registerRootComponent: (app: unknown) => {
        seen.events.push("register");
        seen.registered.push(app);
        try {
          seen.hostAtRegister = host.hostConfig();
        } catch {
          seen.hostAtRegister = null;
        }
      },
    };
  });
});

describe("the entry of the app", () => {
  test("loads the background task before anything else of the app", async () => {
    await import("./index.ts");
    expect(seen.events[0]).toBe("task");
    expect(seen.events).toContain("App");
    expect(seen.events).toContain("register");
  });

  test("gives the host the startup configuration of the app", async () => {
    await import("./index.ts");
    const { startup } = await import("./config/startup.ts");
    const { hostConfig } = await import("./src/native/host.ts");
    expect(hostConfig()).toBe(startup);
  });

  test("gives it before the app is handed to the system, and hands the system the root component", async () => {
    await import("./index.ts");
    const { startup } = await import("./config/startup.ts");
    const { default: App } = await import("./App.tsx");
    expect(seen.events.filter((e) => e === "register")).toHaveLength(1);
    expect(seen.hostAtRegister).toBe(startup);
    expect(seen.registered).toEqual([App]);
  });

  test("is the file that the package names as its entry", () => {
    const manifest = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));
    expect(manifest.main).toBe("index.ts");
  });
});

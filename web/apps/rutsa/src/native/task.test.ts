import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { t } from "../i18n/index.ts";

// The name of the background task is kept by the system across an update of the app: a recording
// that is running when an update is installed goes on under the name it was started with, and a
// task that is defined under another name never receives its positions. The name is written as a
// literal here on purpose.
//
// What the task does with the positions is `storeDelivery` (`core/delivery.test.ts`); this file
// checks the wiring around it.
//
// After `mileage-app` (`src/native/task-name.test.ts`).
const NAME = "org.obop.rutsa.location";

type Body = { data?: { locations?: unknown[] }; error?: { message: string } | null };

const manager = vi.hoisted(() => ({
  defined: [] as { name: string; run: (body: Body) => Promise<void> }[],
}));
vi.mock("expo-task-manager", () => ({
  defineTask: (name: string, run: (body: Body) => Promise<void>) => {
    manager.defined.push({ name, run });
  },
}));

const location = vi.hoisted(() => ({
  started: [] as { name: string; options: unknown }[],
  stopped: [] as string[],
  asked: [] as string[],
  registered: true,
  stopFails: false,
}));
vi.mock("expo-location", () => ({
  Accuracy: { High: 4 },
  startLocationUpdatesAsync: async (name: string, options: unknown) => {
    location.started.push({ name, options });
  },
  stopLocationUpdatesAsync: async (name: string) => {
    location.stopped.push(name);
    if (location.stopFails) throw new Error("the service is gone");
  },
  hasStartedLocationUpdatesAsync: async (name: string) => {
    location.asked.push(name);
    return location.registered;
  },
}));

const store = vi.hoisted(() => ({
  active: null as { id: string } | null,
  appended: [] as { id: string; count: number }[],
  /** How many of the fixes the store keeps; `null` keeps all. */
  keeps: null as number | null,
  fails: null as Error | null,
}));
vi.mock("./store.ts", () => ({
  getStore: async () => {
    if (store.fails !== null) throw store.fails;
    return {
      activeTrip: async () => store.active,
      appendFixes: async (id: string, fixes: unknown[]) => {
        store.appended.push({ id, count: fixes.length });
        return store.keeps ?? fixes.length;
      },
    };
  },
}));

const position = {
  timestamp: 1_000,
  coords: { latitude: 59.3123456, longitude: 18.0654321, accuracy: 5, speed: 1 },
};

/** Imports the task as the entry point does, in a fresh process, and counts the change signals. */
async function loadTask() {
  await import("./task.ts");
  const { onChange } = await import("./bus.ts");
  const changes = { count: 0 };
  onChange(() => {
    changes.count += 1;
  });
  const task = manager.defined[0];
  if (task === undefined) throw new Error("no task was defined");
  return { run: task.run, changes };
}

// The warnings of the task are read by the tests and kept out of their output.
const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);

beforeEach(() => {
  warn.mockClear();
  manager.defined.length = 0;
  location.started.length = 0;
  location.stopped.length = 0;
  location.asked.length = 0;
  location.registered = true;
  location.stopFails = false;
  store.active = null;
  store.appended.length = 0;
  store.keeps = null;
  store.fails = null;
  vi.resetModules();
});

/** Everything the task logged, as one text. */
const logged = () => warn.mock.calls.flat().map(String).join("\n");

describe("the background location task", () => {
  test("is defined once, at import, under the name the system already has", async () => {
    await import("./task.ts");
    expect(manager.defined.map((d) => d.name)).toEqual([NAME]);
  });

  test("stores the fixes of the trip that is recording and tells the screens", async () => {
    store.active = { id: "trip-1" };
    const { run, changes } = await loadTask();
    await run({ data: { locations: [position, position] } });
    expect(store.appended).toEqual([{ id: "trip-1", count: 2 }]);
    expect(changes.count).toBe(1);
    expect(location.stopped).toEqual([]);
  });

  test("fixes that the store drops change nothing the screens show: no signal", async () => {
    store.active = { id: "trip-1" };
    store.keeps = 0;
    const { run, changes } = await loadTask();
    await run({ data: { locations: [position] } });
    expect(store.appended).toEqual([{ id: "trip-1", count: 1 }]);
    expect(changes.count).toBe(0);
  });

  test("with no trip recording it stops the updates under its name and stores nothing", async () => {
    const { run, changes } = await loadTask();
    await run({ data: { locations: [position] } });
    expect(location.stopped).toEqual([NAME]);
    expect(store.appended).toEqual([]);
    expect(changes.count).toBe(0);
  });

  test("a stop that fails is not an error of the task", async () => {
    location.stopFails = true;
    const { run } = await loadTask();
    await expect(run({ data: { locations: [position] } })).resolves.toBeUndefined();
    expect(location.stopped).toEqual([NAME]);
  });

  test("an error of the system or an empty delivery stores nothing and does not open the store", async () => {
    store.active = { id: "trip-1" };
    store.fails = new Error("the store must not be opened");
    const { run, changes } = await loadTask();
    await run({ error: { message: "kCLErrorDomain 0" } });
    await run({ data: { locations: [] } });
    await run({ data: {} });
    await run({});
    expect(store.appended).toEqual([]);
    expect(location.stopped).toEqual([]);
    expect(changes.count).toBe(0);
    expect(logged()).toBe("location task: kCLErrorDomain 0");
  });

  test("a store that cannot be opened is logged, without a coordinate, and the task goes on", async () => {
    store.fails = new Error("disk I/O error");
    const { run, changes } = await loadTask();
    await expect(run({ data: { locations: [position] } })).resolves.toBeUndefined();
    expect(changes.count).toBe(0);
    expect(logged()).toContain("disk I/O error");
    expect(logged()).not.toMatch(/59\.31|18\.06/);
  });
});

describe("starting and stopping the location updates", () => {
  test("start under the name of the task, with the notification of a recording on Android", async () => {
    const recorder = await import("./recorder.ts");
    await recorder.startRecording();
    expect(location.started).toEqual([
      {
        name: NAME,
        options: expect.objectContaining({
          accuracy: 4,
          timeInterval: 5000,
          distanceInterval: 5,
          pausesUpdatesAutomatically: false,
          foregroundService: expect.objectContaining({
            notificationTitle: t("recording.notification.title"),
            notificationBody: t("recording.notification.body"),
          }),
        }),
      },
    ]);
  });

  test("stop asks under the same name and stops a service that runs", async () => {
    const recorder = await import("./recorder.ts");
    await recorder.stopRecording();
    expect(location.asked).toEqual([NAME]);
    expect(location.stopped).toEqual([NAME]);
  });

  test("whether the updates are registered is asked under the same name", async () => {
    const recorder = await import("./recorder.ts");
    await expect(recorder.isRecording()).resolves.toBe(true);
    location.registered = false;
    await expect(recorder.isRecording()).resolves.toBe(false);
    expect(location.asked).toEqual([NAME, NAME]);
  });

  test("a service that is not registered (after Pause, or a restart) is not stopped", async () => {
    location.registered = false;
    const recorder = await import("./recorder.ts");
    await recorder.stopRecording();
    expect(location.asked).toEqual([NAME]);
    expect(location.stopped).toEqual([]);
  });
});

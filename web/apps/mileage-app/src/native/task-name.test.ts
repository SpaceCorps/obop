import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// The name of the background task is kept by the system across an update of the app: a recording
// that is running when an update is installed goes on under the name it was started with, and a
// task that is defined under another name never receives its positions. The name is written as a
// literal here on purpose.

const manager = vi.hoisted(() => ({
  defined: [] as { name: string; run: (body: unknown) => Promise<void> }[],
}));
vi.mock("expo-task-manager", () => ({
  defineTask: (name: string, run: (body: unknown) => Promise<void>) => {
    manager.defined.push({ name, run });
  },
}));

const location = vi.hoisted(() => ({
  started: [] as string[],
  stopped: [] as string[],
  asked: [] as string[],
  registered: true,
}));
vi.mock("expo-location", () => ({
  Accuracy: { High: 4 },
  startLocationUpdatesAsync: async (name: string) => {
    location.started.push(name);
  },
  stopLocationUpdatesAsync: async (name: string) => {
    location.stopped.push(name);
  },
  hasStartedLocationUpdatesAsync: async (name: string) => {
    location.asked.push(name);
    return location.registered;
  },
}));

const store = vi.hoisted(() => ({
  active: null as { clientTripId: string } | null,
  appended: [] as { id: string; count: number }[],
}));
vi.mock("./store.ts", () => ({
  getStore: async () => ({
    activeTrip: async () => store.active,
    appendFixes: async (id: string, fixes: unknown[]) => {
      store.appended.push({ id, count: fixes.length });
      return fixes.length;
    },
  }),
}));
vi.mock("./permissions.ts", () => ({ locationReadiness: async () => "ready" }));

const position = {
  timestamp: 1_000,
  coords: { latitude: 52, longitude: 13, accuracy: 5, speed: 1 },
};

beforeEach(() => {
  manager.defined.length = 0;
  location.started.length = 0;
  location.stopped.length = 0;
  location.asked.length = 0;
  location.registered = true;
  store.active = null;
  store.appended.length = 0;
  vi.resetModules();
});

describe("the background location task", () => {
  test("is defined once, at import, under the name the system already has", async () => {
    await import("./task.ts");
    expect(manager.defined.map((d) => d.name)).toEqual(["obop-mileage-location"]);
  });

  test("stops the service under that name when no trip is recording", async () => {
    await import("./task.ts");
    await manager.defined[0]!.run({ data: { locations: [position] } });
    expect(location.stopped).toEqual(["obop-mileage-location"]);
    expect(store.appended).toEqual([]);
  });

  test("stores the fixes of the trip that is recording", async () => {
    store.active = { clientTripId: "c1" };
    await import("./task.ts");
    await manager.defined[0]!.run({ data: { locations: [position, position] } });
    expect(store.appended).toEqual([{ id: "c1", count: 2 }]);
    expect(location.stopped).toEqual([]);
  });
});

describe("starting, asking and stopping the recording", () => {
  test("all use the name of the task", async () => {
    const recorder = await import("./recorder.ts");
    await recorder.startRecording();
    expect(location.started).toEqual(["obop-mileage-location"]);
    await expect(recorder.isRecording()).resolves.toBe(true);
    await recorder.stopRecording();
    expect(location.asked).toEqual(["obop-mileage-location", "obop-mileage-location"]);
    expect(location.stopped).toEqual(["obop-mileage-location"]);
  });

  test("a service that is not registered is not stopped", async () => {
    location.registered = false;
    const recorder = await import("./recorder.ts");
    await recorder.stopRecording();
    expect(location.stopped).toEqual([]);
  });
});

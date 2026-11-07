import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// The answers of the two systems that matter here (read from expo-location 58.0.18):
// - iOS: with Location Services switched off for the whole phone the status of the app reads
//   "denied" (and `canAskAgain` is false), the same as a refusal; the switch is a separate question.
// - Android: with the switch off the permission still reads "granted".
const location = vi.hoisted(() => {
  const state = {
    permission: { granted: false, canAskAgain: true } as {
      granted: boolean;
      canAskAgain: boolean;
      ios?: { accuracy: string };
      android?: { accuracy: string };
    },
    requested: { granted: false, canAskAgain: false } as {
      granted: boolean;
      canAskAgain: boolean;
    },
    servicesEnabled: true,
    servicesAsked: 0,
  };
  return {
    state,
    getForegroundPermissionsAsync: async () => state.permission,
    requestForegroundPermissionsAsync: async () => state.requested,
    hasServicesEnabledAsync: async () => {
      state.servicesAsked += 1;
      return state.servicesEnabled;
    },
  };
});
vi.mock("expo-location", () => location);

const platform = vi.hoisted(() => ({ OS: "ios" as string, Version: 27 }));
vi.mock("react-native", () => ({
  Platform: platform,
  Linking: { openSettings: async () => undefined },
  PermissionsAndroid: { request: async () => "granted", RESULTS: { GRANTED: "granted" } },
}));

const { locationAccess, locationReadiness, requestLocation } = await import("./permissions.ts");

const { state } = location;
beforeEach(() => {
  state.permission = { granted: false, canAskAgain: true };
  state.requested = { granted: false, canAskAgain: false };
  state.servicesEnabled = true;
  state.servicesAsked = 0;
});

describe("iOS", () => {
  beforeEach(() => {
    platform.OS = "ios";
  });

  test("Location Services off is named before the refusal that iOS reports with it", async () => {
    state.permission = { granted: false, canAskAgain: false };
    state.servicesEnabled = false;
    expect(await locationReadiness()).toBe("services_off");
    expect(await locationAccess()).toBe("services_off");
  });

  test("a granted permission with the switch off is the same", async () => {
    state.permission = { granted: true, canAskAgain: true, ios: { accuracy: "full" } };
    state.servicesEnabled = false;
    expect(await locationReadiness()).toBe("services_off");
    expect(await locationAccess()).toBe("services_off");
  });

  test("with the switch on, a refusal is a refusal", async () => {
    state.permission = { granted: false, canAskAgain: false };
    expect(await locationReadiness()).toBe("no_permission");
    expect(await locationAccess()).toBe("blocked");
    state.permission = { granted: false, canAskAgain: true };
    expect(await locationAccess()).toBe("ask");
  });

  test("precise and switched on is ready; a reduced accuracy is approximate", async () => {
    state.permission = { granted: true, canAskAgain: true, ios: { accuracy: "full" } };
    expect(await locationReadiness()).toBe("ready");
    expect(await locationAccess()).toBe("granted");
    state.permission = { granted: true, canAskAgain: true, ios: { accuracy: "reduced" } };
    expect(await locationReadiness()).toBe("approximate");
    expect(await locationAccess()).toBe("approximate");
  });

  test("the answer to the system dialog is checked against the switch as well", async () => {
    state.requested = { granted: false, canAskAgain: false };
    state.servicesEnabled = false;
    expect(await requestLocation()).toBe("services_off");
    state.servicesEnabled = true;
    expect(await requestLocation()).toBe("blocked");
  });
});

describe("Android (unchanged)", () => {
  beforeEach(() => {
    platform.OS = "android";
  });

  test("a refusal is named first and the switch is not even asked", async () => {
    state.permission = { granted: false, canAskAgain: false };
    state.servicesEnabled = false;
    expect(await locationReadiness()).toBe("no_permission");
    expect(await locationAccess()).toBe("blocked");
    expect(state.servicesAsked).toBe(0);
  });

  test("a granted permission with the switch off is services_off", async () => {
    state.permission = { granted: true, canAskAgain: true, android: { accuracy: "fine" } };
    state.servicesEnabled = false;
    expect(await locationReadiness()).toBe("services_off");
  });

  test("the answer of the access check does not look at the switch", async () => {
    state.permission = { granted: true, canAskAgain: true, android: { accuracy: "fine" } };
    state.servicesEnabled = false;
    expect(await locationAccess()).toBe("granted");
    expect(await requestLocation()).toBe("blocked");
  });
});

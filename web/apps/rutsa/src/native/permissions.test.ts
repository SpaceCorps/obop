import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// The answers of the two systems that matter here (read from expo-location 58.0.18):
// - iOS: with Location Services switched off for the whole phone the status of the app reads
//   "denied" (and `canAskAgain` is false), the same as a refusal; the switch is a separate question.
// - Android: with the switch off the permission still reads "granted".
//
// After `mileage-app` (`src/native/permissions.test.ts`).
type Permission = {
  granted: boolean;
  canAskAgain: boolean;
  ios?: { accuracy: string };
  android?: { accuracy: string };
};

const location = vi.hoisted(() => {
  const state = {
    permission: { granted: false, canAskAgain: true } as Permission,
    requested: { granted: false, canAskAgain: false } as Permission,
    servicesEnabled: true,
    servicesAsked: 0,
    dialogs: 0,
  };
  return {
    state,
    getForegroundPermissionsAsync: async () => state.permission,
    requestForegroundPermissionsAsync: async () => {
      state.dialogs += 1;
      return state.requested;
    },
    hasServicesEnabledAsync: async () => {
      state.servicesAsked += 1;
      return state.servicesEnabled;
    },
  };
});
vi.mock("expo-location", () => location);

const native = vi.hoisted(() => ({
  platform: { OS: "ios" as string, Version: 17 as number | string },
  notificationAnswer: "granted",
  notificationRequests: [] as string[],
  settingsOpened: 0,
}));
vi.mock("react-native", () => ({
  Platform: native.platform,
  Linking: {
    openSettings: async () => {
      native.settingsOpened += 1;
    },
  },
  PermissionsAndroid: {
    request: async (permission: string) => {
      native.notificationRequests.push(permission);
      return native.notificationAnswer;
    },
    RESULTS: { GRANTED: "granted" },
  },
}));

const { locationAccess, locationReadiness, openSettings, requestLocation, requestNotifications } =
  await import("./permissions.ts");

const { state } = location;
beforeEach(() => {
  state.permission = { granted: false, canAskAgain: true };
  state.requested = { granted: false, canAskAgain: false };
  state.servicesEnabled = true;
  state.servicesAsked = 0;
  state.dialogs = 0;
  native.notificationAnswer = "granted";
  native.notificationRequests.length = 0;
  native.settingsOpened = 0;
});

describe("iOS", () => {
  beforeEach(() => {
    native.platform.OS = "ios";
    native.platform.Version = "17.4";
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
    state.requested = { granted: true, canAskAgain: true, ios: { accuracy: "full" } };
    expect(await requestLocation()).toBe("granted");
    expect(state.dialogs).toBe(3);
  });

  test("the notification of a recording needs no permission: nothing is asked", async () => {
    expect(await requestNotifications()).toBe(true);
    expect(native.notificationRequests).toEqual([]);
  });
});

describe("Android", () => {
  beforeEach(() => {
    native.platform.OS = "android";
    native.platform.Version = 34;
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

  test("Approximate is approximate, also with the switch on", async () => {
    state.permission = { granted: true, canAskAgain: true, android: { accuracy: "coarse" } };
    expect(await locationReadiness()).toBe("approximate");
    expect(await locationAccess()).toBe("approximate");
  });

  test("the answer of the access check does not look at the switch", async () => {
    state.permission = { granted: true, canAskAgain: true, android: { accuracy: "fine" } };
    state.servicesEnabled = false;
    expect(await locationAccess()).toBe("granted");
    expect(await requestLocation()).toBe("blocked");
  });

  test("Android 13 and later asks for the notification, and a refusal is told", async () => {
    expect(await requestNotifications()).toBe(true);
    native.notificationAnswer = "denied";
    expect(await requestNotifications()).toBe(false);
    native.notificationAnswer = "never_ask_again";
    expect(await requestNotifications()).toBe(false);
    expect(native.notificationRequests).toEqual([
      "android.permission.POST_NOTIFICATIONS",
      "android.permission.POST_NOTIFICATIONS",
      "android.permission.POST_NOTIFICATIONS",
    ]);
  });

  test("before Android 13 the notification needs no permission: nothing is asked", async () => {
    native.platform.Version = 32;
    expect(await requestNotifications()).toBe(true);
    expect(native.notificationRequests).toEqual([]);
  });
});

test("the settings of the app are opened by the system", async () => {
  await openSettings();
  expect(native.settingsOpened).toBe(1);
});

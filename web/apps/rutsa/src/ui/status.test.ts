import { describe, expect, test } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { bannerShown, bannerText, problemsOf, settingsFor, type Problem } from "./status.ts";

describe("what keeps the phone from recording well", () => {
  test("nothing when it can record and show the recording", () => {
    expect(problemsOf({ readiness: "ready", notifications: true })).toEqual([]);
  });

  test("each case of the location is its own problem", () => {
    expect(problemsOf({ readiness: "services_off", notifications: true })).toEqual([
      "location_off",
    ]);
    expect(problemsOf({ readiness: "no_permission", notifications: true })).toEqual([
      "no_permission",
    ]);
    expect(problemsOf({ readiness: "approximate", notifications: true })).toEqual(["approximate"]);
  });

  test("the notifications alone, and both, the location first", () => {
    expect(problemsOf({ readiness: "ready", notifications: false })).toEqual(["notifications_off"]);
    expect(problemsOf({ readiness: "services_off", notifications: false })).toEqual([
      "location_off",
      "notifications_off",
    ]);
  });
});

describe("whether the banner shows", () => {
  const none = new Set<Problem>();

  test("it shows while there is a problem, and not without one", () => {
    expect(bannerShown(["location_off"], none)).toBe(true);
    expect(bannerShown([], none)).toBe(false);
  });

  test("a closed banner stays closed while the same problems are there", () => {
    expect(bannerShown(["notifications_off"], new Set(["notifications_off"]))).toBe(false);
    expect(
      bannerShown(
        ["location_off", "notifications_off"],
        new Set<Problem>(["location_off", "notifications_off"]),
      ),
    ).toBe(false);
  });

  test("a problem that comes after the banner was closed shows it again", () => {
    expect(
      bannerShown(["location_off", "notifications_off"], new Set<Problem>(["notifications_off"])),
    ).toBe(true);
  });

  test("a problem that went away leaves the others closed", () => {
    expect(
      bannerShown(["notifications_off"], new Set<Problem>(["location_off", "notifications_off"])),
    ).toBe(false);
  });
});

describe("Open settings", () => {
  test("opens the location settings while location is off on the phone", () => {
    expect(settingsFor(["location_off"])).toBe("location");
    expect(settingsFor(["location_off", "notifications_off"])).toBe("location");
  });

  test("opens the app's own page for its permission and its notifications", () => {
    expect(settingsFor(["no_permission"])).toBe("app");
    expect(settingsFor(["approximate", "notifications_off"])).toBe("app");
    expect(settingsFor(["notifications_off"])).toBe("app");
  });
});

describe("the one sentence of the banner", () => {
  const text = (problems: Problem[]) => {
    const key = bannerText(problems);
    return key === null ? null : en[key];
  };

  test("each problem alone, as an ordinary sentence", () => {
    expect(text(["location_off"])).toBe("Location is off.");
    expect(text(["no_permission"])).toBe("Location access is off.");
    expect(text(["approximate"])).toBe("Precise location is off.");
    expect(text(["notifications_off"])).toBe("Notifications are off.");
  });

  test("the location and the notifications in one sentence", () => {
    expect(text(["location_off", "notifications_off"])).toBe("Location and notifications are off.");
    expect(text(["no_permission", "notifications_off"])).toBe(
      "Location access and notifications are off.",
    );
    expect(text(["approximate", "notifications_off"])).toBe(
      "Precise location and notifications are off.",
    );
  });

  test("the order of the problems does not change the sentence", () => {
    expect(text(["notifications_off", "location_off"])).toBe("Location and notifications are off.");
  });

  test("no sentence without a problem", () => {
    expect(bannerText([])).toBeNull();
  });
});

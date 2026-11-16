// The small pure pieces: Back, the flags of the build, the ground, the stored record.
import { describe, expect, test } from "vite-plus/test";
import { backAction } from "./back.ts";
import { isTestBuild, readBuild } from "./build.ts";
import {
  NEW_FOR_MS,
  dayText,
  decodeServerRecord,
  encodeServerRecord,
  makeServerRecord,
  newSince,
} from "./server-record.ts";
import { barStyle, groundOf, schemeOfSystem, startGround } from "./theme.ts";

describe("Android's Back", () => {
  test("closes This app first", () => {
    for (const canGoBack of [true, false]) {
      for (const errorShown of [true, false]) {
        expect(backAction({ settingsOpen: true, errorShown, canGoBack })).toBe("close_settings");
      }
    }
  });

  test("walks the page's history while there is some", () => {
    expect(backAction({ settingsOpen: false, errorShown: false, canGoBack: true })).toBe(
      "web_back",
    );
  });

  test("leaves the app at the root of the history", () => {
    expect(backAction({ settingsOpen: false, errorShown: false, canGoBack: false })).toBe("leave");
  });

  test("does not walk the history from under the error screen", () => {
    expect(backAction({ settingsOpen: false, errorShown: true, canGoBack: true })).toBe("leave");
  });
});

describe("the flags of the build", () => {
  test("are read from the extra of the Expo config", () => {
    expect(
      readBuild({
        cleartextAllowed: true,
        webviewDebug: true,
        defaultServer: " http://localhost:18180 ",
      }),
    ).toEqual({ cleartext: true, webviewDebug: true, defaultServer: "http://localhost:18180" });
    expect(readBuild({ cleartextAllowed: false, webviewDebug: false, defaultServer: "" })).toEqual({
      cleartext: false,
      webviewDebug: false,
      defaultServer: "",
    });
  });

  test("only the value true switches something on", () => {
    const plain = { cleartext: false, webviewDebug: false, defaultServer: "" };
    for (const extra of [
      undefined,
      null,
      "x",
      7,
      [],
      {},
      { cleartextAllowed: "true", webviewDebug: 1, defaultServer: 5 },
      { cleartextAllowed: "1", webviewDebug: "yes", defaultServer: null },
    ]) {
      expect(readBuild(extra), JSON.stringify(extra)).toEqual(plain);
    }
  });

  test("a test build is one with either switch", () => {
    const build = (cleartext: boolean, webviewDebug: boolean) => ({
      cleartext,
      webviewDebug,
      defaultServer: "",
    });
    expect(isTestBuild(build(false, false))).toBe(false);
    expect(isTestBuild(build(true, false))).toBe(true);
    expect(isTestBuild(build(false, true))).toBe(true);
    expect(isTestBuild(build(true, true))).toBe(true);
    // A default server alone does not make one.
    expect(isTestBuild({ ...build(false, false), defaultServer: "https://erp.example.com" })).toBe(
      false,
    );
  });
});

describe("the ground and the status bar", () => {
  test("anything but dark is light", () => {
    expect(schemeOfSystem("dark")).toBe("dark");
    for (const system of ["light", "unspecified", "", null, undefined]) {
      expect(schemeOfSystem(system)).toBe("light");
    }
  });

  test("before the page has reported, the ground is the page's own token for the system's scheme", () => {
    expect(startGround("light")).toMatch(/^#[0-9a-f]{6}$/i);
    expect(startGround("dark")).toMatch(/^#[0-9a-f]{6}$/i);
    expect(startGround("light")).not.toBe(startGround("dark"));
    expect(groundOf(null, "dark")).toBe(startGround("dark"));
  });

  test("afterwards it is what the page reported, if that is a colour", () => {
    expect(groundOf({ scheme: "dark", background: "#0a0c11" }, "light")).toBe("#0a0c11");
    for (const background of ["red", "#fff", "url(x)", "#0a0c11; x", ""]) {
      expect(groundOf({ scheme: "dark", background }, "light"), background).toBe(
        startGround("light"),
      );
    }
  });

  test("light icons on a dark ground, dark ones on a light ground", () => {
    expect(barStyle("dark")).toBe("light");
    expect(barStyle("light")).toBe("dark");
  });
});

describe("the stored record", () => {
  const record = makeServerRecord(
    { origin: "https://erp.example.com", path: "/admin/" },
    "0.2.1",
    1_763_200_000_000,
  );

  test("is read back as it was written", () => {
    expect(record).toEqual({
      v: 1,
      origin: "https://erp.example.com",
      path: "/admin/",
      coreVersion: "0.2.1",
      addedAt: 1_763_200_000_000,
    });
    expect(decodeServerRecord(encodeServerRecord(record))).toEqual(record);
    expect(
      decodeServerRecord(
        encodeServerRecord({ ...record, origin: "http://[::1]:18180", path: "/" }),
      ),
    ).toMatchObject({ origin: "http://[::1]:18180", path: "/" });
  });

  test("holds nothing but the five fields", () => {
    const text = encodeServerRecord({ ...record, token: "secret" } as typeof record);
    expect(Object.keys(JSON.parse(text) as object).sort()).toEqual([
      "addedAt",
      "coreVersion",
      "origin",
      "path",
      "v",
    ]);
    expect(text).not.toContain("secret");
  });

  test("an item that is not a record is none", () => {
    const item = (changes: Record<string, unknown>) => JSON.stringify({ ...record, ...changes });
    for (const text of [
      null,
      undefined,
      "",
      "{",
      "null",
      "[]",
      '"https://erp.example.com"',
      item({ v: 2 }),
      item({ v: "1" }),
      // An origin as `originOf` does not write it.
      item({ origin: "https://ERP.example.com" }),
      item({ origin: "https://erp.example.com/" }),
      item({ origin: "https://erp.example.com:443" }),
      item({ origin: "https://user@erp.example.com" }),
      item({ origin: "erp.example.com" }),
      item({ origin: "javascript:alert(1)" }),
      item({ origin: 7 }),
      // A path that is not `/` or one segment between slashes.
      item({ path: "/admin" }),
      item({ path: "admin/" }),
      item({ path: "/a/b/" }),
      item({ path: "/Admin/" }),
      item({ path: "/../" }),
      item({ path: "" }),
      item({ coreVersion: 3 }),
      item({ coreVersion: "1".repeat(65) }),
      item({ addedAt: "now" }),
      item({ addedAt: -1 }),
      item({ addedAt: null }),
      item({ padding: "x".repeat(3_000) }),
    ]) {
      expect(decodeServerRecord(text), String(text).slice(0, 80)).toBeNull();
    }
  });

  test("the day it was stored, in the phone's own time", () => {
    const noon = new Date(2026, 10, 15, 12, 0, 0).getTime();
    expect(dayText(noon)).toBe("15 Nov 2026");
    expect(dayText(new Date(2027, 0, 1, 0, 0, 1).getTime())).toBe("1 Jan 2027");
  });

  test("the host bar says new since for seven days, and not for a record of the future", () => {
    const added = new Date(2026, 10, 15, 12, 0, 0).getTime();
    const stored = { ...record, addedAt: added };
    expect(newSince(stored, added)).toBe("15 Nov 2026");
    expect(newSince(stored, added + NEW_FOR_MS - 1)).toBe("15 Nov 2026");
    expect(newSince(stored, added + NEW_FOR_MS)).toBeNull();
    expect(newSince(stored, added - 1)).toBeNull();
  });
});

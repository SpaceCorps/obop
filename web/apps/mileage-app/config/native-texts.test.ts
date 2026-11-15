import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";
import { DEFAULT_LANGUAGE, NATIVE_TEXTS, expoLocales, type NativeTexts } from "./native-texts.ts";
import { LANGUAGE_CODES } from "../src/i18n/languages.ts";

describe("the native texts", () => {
  test("every language of the app has them, in the same shape, none empty", () => {
    expect(Object.keys(NATIVE_TEXTS).sort()).toEqual([...LANGUAGE_CODES].sort());
    expect(DEFAULT_LANGUAGE).toBe("en");
    for (const [language, texts] of Object.entries(NATIVE_TEXTS)) {
      expect(Object.keys(texts).sort(), language).toEqual([
        "appName",
        "localNetwork",
        "locationWhenInUse",
      ]);
      for (const [key, text] of Object.entries(texts)) {
        expect(text.trim(), `${language} ${key}`).not.toBe("");
        expect(text, `${language} ${key}`).not.toMatch(/[{}]/);
      }
    }
  });

  test("the Swedish texts are not copies of the English ones, except the name of the app", () => {
    expect(NATIVE_TEXTS.sv.appName).toBe(NATIVE_TEXTS.en.appName);
    for (const key of ["locationWhenInUse", "localNetwork"] as (keyof NativeTexts)[]) {
      expect(NATIVE_TEXTS.sv[key], key).not.toBe(NATIVE_TEXTS.en[key]);
    }
  });

  test("the Expo locales have a Swedish entry for iOS and Android, and none for the default language", () => {
    const locales = expoLocales();
    expect(Object.keys(locales)).toEqual(["sv"]);
    expect(locales.sv).toEqual({
      ios: {
        CFBundleDisplayName: NATIVE_TEXTS.sv.appName,
        NSLocationWhenInUseUsageDescription: NATIVE_TEXTS.sv.locationWhenInUse,
        NSLocalNetworkUsageDescription: NATIVE_TEXTS.sv.localNetwork,
      },
      android: { app_name: NATIVE_TEXTS.sv.appName },
    });
  });
});

// The real Expo CLI on the real app.config.ts, as in app-config.test.ts.
const appDir = fileURLToPath(new URL("..", import.meta.url));
const expoCli = createRequire(import.meta.url).resolve("expo/bin/cli");

describe("the configuration that expo prebuild gets", { timeout: 30_000 }, () => {
  const result = spawnSync(
    process.execPath,
    [expoCli, "config", "--type", "introspect", "--json", appDir],
    { cwd: appDir, env: { ...process.env, EXPO_NO_TELEMETRY: "1" }, encoding: "utf8" },
  );
  const config = JSON.parse(result.stdout) as {
    name: string;
    locales: Record<string, { ios: Record<string, string>; android: Record<string, string> }>;
    ios: { infoPlist: Record<string, unknown> };
  };

  test("the English texts of the build are the English native texts", () => {
    expect(result.status, result.stderr).toBe(0);
    expect(config.name).toBe(NATIVE_TEXTS.en.appName);
    expect(config.ios.infoPlist.NSLocationWhenInUseUsageDescription).toBe(
      NATIVE_TEXTS.en.locationWhenInUse,
    );
    expect(config.ios.infoPlist.NSLocalNetworkUsageDescription).toBe(NATIVE_TEXTS.en.localNetwork);
  });

  test("the Swedish texts are in the locales of the config", () => {
    expect(config.locales).toEqual(expoLocales());
    expect(config.locales.sv?.ios.NSLocationWhenInUseUsageDescription).toMatch(
      /^obop Mileage registrerar/,
    );
    expect(config.locales.sv?.android.app_name).toBe("obop Mileage");
  });

  test("every usage text the build declares has a Swedish one, and the Swedish list has no other", () => {
    const declared = Object.keys(config.ios.infoPlist)
      .filter((key) => /^NS\w+UsageDescription$/.test(key))
      .sort();
    expect(declared).toEqual([
      "NSLocalNetworkUsageDescription",
      "NSLocationWhenInUseUsageDescription",
    ]);
    const swedish = Object.keys(config.locales.sv!.ios)
      .filter((key) => /^NS\w+UsageDescription$/.test(key))
      .sort();
    expect(swedish).toEqual(declared);
  });
});

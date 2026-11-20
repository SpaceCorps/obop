// The only module that imports vite-plus (config factory); apps import from here.
import { defineConfig as define, type UserConfig } from "vite-plus";

/**
 * The per-test time limit of `vp test` in every app and package, unless a config sets its own.
 * Vitest's default of 5 s is too short on a loaded Windows machine for the tests that spawn a
 * shell (the iOS and emulator scripts of the phone apps), open the native stores of the Mileage
 * app or walk the source tree (the i18n and brand guards): they pass alone in about a second but
 * hit 5 s while a cargo build runs beside them (docs/ISSUES.md UB12, U22, U49, U64, U72). A test
 * that hangs still fails, after 20 s.
 */
export const TEST_TIMEOUT_MS = 20_000;

export function defineConfig(config: UserConfig) {
  return define({ ...config, test: { testTimeout: TEST_TIMEOUT_MS, ...config.test } });
}

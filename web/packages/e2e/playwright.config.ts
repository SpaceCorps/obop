import { mkdtempSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig } from "@playwright/test";
import { requireBuilt } from "./support/env.ts";

async function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const s = createServer();
    s.once("error", fail);
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address() as { port: number };
      s.close(() => ok(port));
    });
  });
}

// The first process to load this file (the runner) picks a fresh temp directory and free ports and
// exports them; the workers it spawns inherit the variables, so everyone agrees on them.
requireBuilt();
process.env.OBOP_E2E_DIR ??= mkdtempSync(join(tmpdir(), "obop-e2e-"));
process.env.OBOP_E2E_APP_PORT ??= String(await freePort());
process.env.OBOP_E2E_PLATFORM_PORT ??= String(await freePort());
const app = `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`;
const platform = `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;

// Browser: bundled Chromium in CI; locally the system Chrome, because Playwright's browsers are not
// downloaded. PW_CHROME_PATH points at a specific executable, PW_CHANNEL picks another channel.
const local = !process.env.CI;
const browser = process.env.PW_CHROME_PATH
  ? { launchOptions: { executablePath: process.env.PW_CHROME_PATH } }
  : process.env.PW_CHANNEL
    ? { channel: process.env.PW_CHANNEL }
    : local
      ? { channel: "chrome" }
      : {};

export default defineConfig({
  testDir: "./tests",
  outputDir: "./test-results",
  // One instance and one database back the whole run, so specs run one at a time; each spec
  // creates its own uniquely named data and restores any instance-wide setting it changes.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 30_000,
  expect: { timeout: 7_000 },
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "playwright-report" }]]
    : [["list"]],
  use: {
    baseURL: app,
    viewport: { width: 1440, height: 900 },
    colorScheme: "light",
    locale: "en-US",
    timezoneId: "UTC",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    ...browser,
  },
  globalTeardown: "./support/global-teardown.ts",
  webServer: [
    {
      command: "node support/serve.ts app",
      url: `${app}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      command: "node support/serve.ts platform",
      url: `${platform}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
  ],
});

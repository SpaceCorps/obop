import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig } from "@playwright/test";
import { requireBuilt } from "./support/env.ts";
import { assignPorts } from "./support/ports.ts";

// The first process to load this file (the runner) picks a fresh temp directory and free ports and
// exports them; the workers it spawns inherit the variables, so everyone agrees on them. The ports
// are picked together (see support/ports.ts), so no two servers of the run can be given the same one.
requireBuilt();
process.env.OBOP_E2E_DIR ??= mkdtempSync(join(tmpdir(), "obop-e2e-"));
await assignPorts();
const app = `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`;
const platform = `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;
const shop = `http://127.0.0.1:${process.env.OBOP_E2E_SHOP_PORT}`;
const market = `http://127.0.0.1:${process.env.OBOP_E2E_MARKET_PORT}`;

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
    actionTimeout: 10_000,
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
      // A second instance reached as localhost: passkeys (WebAuthn) need a host name, and the
      // other instance's public URL is 127.0.0.1.
      command: "node support/serve.ts passkeys",
      url: `http://127.0.0.1:${process.env.OBOP_E2E_PASSKEY_PORT}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      // A third instance whose default language is Swedish (OBOP_DEFAULT_LANGUAGE=sv), for the
      // sign-in screens that come before the instance settings can be read.
      command: "node support/serve.ts language",
      url: `http://127.0.0.1:${process.env.OBOP_E2E_LANGUAGE_PORT}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      // An instance with a brand set (name, message and logo), for the OAuth pages: the logo,
      // the tab icon and the content security policy that has to allow them.
      command: "node support/serve.ts brand",
      url: `http://127.0.0.1:${process.env.OBOP_E2E_BRAND_PORT}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      // A stand-in for Microsoft Entra (discovery, keys, a page to pick who signs in, the token
      // endpoint) and an instance that trusts it as its Microsoft authority (D91).
      command: "node support/serve.ts idp",
      url: `http://127.0.0.1:${process.env.OBOP_E2E_IDP_PORT}/health`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      command: "node support/serve.ts sso",
      url: `http://127.0.0.1:${process.env.OBOP_E2E_SSO_PORT}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      // A tiny marketplace the update specs fill with newer versions of their modules.
      command: "node support/serve.ts market",
      url: `${market}/index.json`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 60_000,
    },
    {
      command: "node support/serve.ts shop",
      url: `${shop}/`,
      reuseExistingServer: false,
      gracefulShutdown: { signal: "SIGTERM", timeout: 5_000 },
      stdout: "pipe",
      stderr: "pipe",
      timeout: 120_000,
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

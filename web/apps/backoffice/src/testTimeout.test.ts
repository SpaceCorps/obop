// Every app and package of web/ takes its vite config from web/config, which gives `vp test` a
// 20 s limit per test instead of vitest's 5 s (docs/ISSUES.md UB12, U22, U49, U64, U72: the tests
// that spawn a shell or walk the tree time out on a loaded Windows machine). This keeps every
// config on that factory, so no app falls back to the 5 s default.
import { readdirSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { join } from "node:path";
import { describe, expect, it } from "vite-plus/test";

const web = fileURLToPath(new URL("../../..", import.meta.url));
const configs = ["apps", "packages"].flatMap((dir) =>
  readdirSync(join(web, dir))
    .map((name) => join(web, dir, name, "vite.config.ts"))
    .filter((file) => existsSync(file)),
);

describe("the shared vite config", () => {
  it("finds the configs", () => {
    expect(configs.length).toBeGreaterThan(8);
  });

  it.each(configs)("%s gives each test at least 20 s", async (file) => {
    const config = (await import(pathToFileURL(file).href)).default;
    expect(config.test?.testTimeout).toBeGreaterThanOrEqual(20_000);
  });

  it("does not override a limit a config sets itself", async () => {
    const { defineConfig } = await import("../../../config/index.ts");
    expect(defineConfig({ test: { testTimeout: 45_000 } }).test?.testTimeout).toBe(45_000);
  });
});

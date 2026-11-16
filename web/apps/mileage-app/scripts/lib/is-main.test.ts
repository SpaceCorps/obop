import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { isMain } from "./is-main.mjs";

const here = fileURLToPath(import.meta.url);
const routeSource = fileURLToPath(new URL("../emulator/route.mjs", import.meta.url));
const libSource = fileURLToPath(new URL("./is-main.mjs", import.meta.url));

describe("isMain", () => {
  test("is true for the file Node was started with and false for any other", () => {
    expect(isMain(pathToFileURL(here).href, here)).toBe(true);
    expect(isMain(pathToFileURL(here).href, routeSource)).toBe(false);
  });

  test("is false when there is no entry file or it does not exist", () => {
    expect(isMain(pathToFileURL(here).href, undefined)).toBe(false);
    expect(isMain(pathToFileURL(here).href, "")).toBe(false);
    expect(isMain(pathToFileURL(here).href, join(tmpdir(), "obop-no-such-file.mjs"))).toBe(false);
  });
});

describe("the route script, started and imported", () => {
  const dir = mkdtempSync(join(tmpdir(), "obop route "));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "emulator"));
  mkdirSync(join(dir, "lib"));
  copyFileSync(routeSource, join(dir, "emulator", "route.mjs"));
  copyFileSync(libSource, join(dir, "lib", "is-main.mjs"));
  const copy = join(dir, "emulator", "route.mjs");

  test("prints the lengths when started, also from a path with a space", () => {
    const result = spawnSync(process.execPath, [copy], { encoding: "utf8" });
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("waypoints: 41");
    expect(result.stdout).toContain("3989.44 m");
    expect(result.stdout).toContain("4000.00 m");
  });

  test("prints nothing when another script imports it", () => {
    const script = `import { waypoints } from ${JSON.stringify(pathToFileURL(copy).href)}; console.log(waypoints().length);`;
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      encoding: "utf8",
    });
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe("41");
  });

  test("prints the lengths when started through a link", (context) => {
    const link = join(dir, "linked-route.mjs");
    try {
      symlinkSync(copy, link);
    } catch {
      // Windows lets only an administrator make links: the case is proved on the Mac.
      context.skip();
    }
    const result = spawnSync(process.execPath, [link], { encoding: "utf8" });
    expect(result.stdout).toContain("waypoints: 41");
  });
});

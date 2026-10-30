import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";

const script = fileURLToPath(new URL("./env.sh", import.meta.url));
const shell = spawnSync("sh", ["-c", "exit 0"]);
const hasShell = shell.status === 0;

/**
 * A minimal environment, so that nothing of the caller's (CI, OBOP_*) leaks into the shell under
 * test. NODE_ENV is there because Expo's types make it a required key of the environment.
 */
const cleanEnv = (extra: Record<string, string>): NodeJS.ProcessEnv => ({
  NODE_ENV: "test",
  PATH: process.env.PATH ?? "",
  ...extra,
});

describe.skipIf(!hasShell)("scripts/ios/env.sh", () => {
  const home = mkdtempSync(join(tmpdir(), "obop-env-"));
  // The home folder as `sh` writes it (Git Bash on Windows turns C:\Users into /c/Users).
  const shHome = spawnSync("sh", ["-c", 'cd "$HOME" && pwd'], {
    encoding: "utf8",
    env: cleanEnv({ HOME: home }),
  }).stdout.trim();
  afterAll(() => rmSync(home, { recursive: true, force: true }));

  /** Sources the file in a fresh `sh` with the given variables and prints `names` one per line. */
  function sourced(names: string[], env: Record<string, string>): string[] {
    const print = names.map((n) => `printf '%s\\n' "\${${n}-<unset>}"`).join("; ");
    const result = spawnSync("sh", ["-c", `. "${script.replaceAll("\\", "/")}"; ${print}`], {
      encoding: "utf8",
      env: cleanEnv({ HOME: home, ...env }),
    });
    expect(result.status, result.stderr).toBe(0);
    return result.stdout.trimEnd().split("\n");
  }

  test("CI is not set afterwards, whatever it was before: Expo refuses its device question under CI", () => {
    expect(sourced(["CI"], { CI: "1" })).toEqual(["<unset>"]);
  });

  test("it sets the language, the caches and the tools under OBOP_TOOLS, default ~/build/tools", () => {
    const [gemHome, lang, store, path] = sourced(
      ["GEM_HOME", "LANG", "npm_config_store_dir", "PATH"],
      {},
    );
    expect(gemHome).toBe(`${shHome}/build/tools/gems`);
    expect(lang).toBe("en_US.UTF-8");
    expect(store).toBe(`${shHome}/build/tools/pnpm-store`);
    expect(path!.startsWith(`${shHome}/build/tools/gems/bin:`)).toBe(true);
    expect(sourced(["GEM_HOME"], { OBOP_TOOLS: "/opt/t" })).toEqual(["/opt/t/gems"]);
  });

  test("it sets no variable that changes the app: the only OBOP_ variable it exports is OBOP_BUILD_ROOT", () => {
    const result = spawnSync("sh", ["-c", `. "${script.replaceAll("\\", "/")}"; env`], {
      encoding: "utf8",
      env: cleanEnv({ HOME: home }),
    });
    expect(result.status, result.stderr).toBe(0);
    const exported = result.stdout
      .split("\n")
      .map((line) => /^(OBOP_[A-Z0-9_]*)=/.exec(line)?.[1])
      .filter((name): name is string => name !== undefined);
    expect(exported).toEqual(["OBOP_BUILD_ROOT"]);
    // And the named ones are unset, in the shell as well.
    const names = [
      "OBOP_ALLOW_CLEARTEXT",
      "OBOP_DEFAULT_SERVER",
      "OBOP_IOS_BUNDLE_ID",
      "OBOP_RECORD_THINNING",
      "OBOP_DEV_CLIENT",
    ];
    expect(sourced(names, {})).toEqual(names.map(() => "<unset>"));
  });

  test("the folder of the drive is private to its owner", () => {
    const [root] = sourced(["OBOP_BUILD_ROOT"], {});
    expect(root).toBe(`${shHome}/.obop-mileage-build`);
    if (process.platform !== "win32") {
      expect(statSync(join(home, ".obop-mileage-build")).mode & 0o777).toBe(0o700);
    }
  });
});

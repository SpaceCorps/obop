import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";

const script = fileURLToPath(new URL("./env.sh", import.meta.url));
const hasShell = spawnSync("sh", ["-c", "exit 0"]).status === 0;

/** A minimal environment, so that nothing of the caller's (OBOP_*) leaks into the shell under test. */
const cleanEnv = (extra: Record<string, string>): NodeJS.ProcessEnv => ({
  NODE_ENV: "test",
  PATH: process.env.PATH ?? "",
  ...extra,
});

describe.skipIf(!hasShell)("scripts/ios/env.sh", () => {
  const home = mkdtempSync(join(tmpdir(), "obop-host-env-"));
  const shHome = spawnSync("sh", ["-c", 'cd "$HOME" && pwd'], {
    encoding: "utf8",
    env: cleanEnv({ HOME: home }),
  }).stdout.trim();
  afterAll(() => rmSync(home, { recursive: true, force: true }));

  function sourced(names: string[], env: Record<string, string>): string[] {
    const print = names.map((n) => `printf '%s\\n' "\${${n}-<unset>}"`).join("; ");
    const result = spawnSync("sh", ["-c", `. "${script.replaceAll("\\", "/")}"; ${print}`], {
      encoding: "utf8",
      env: cleanEnv({ HOME: home, ...env }),
    });
    expect(result.status, result.stderr).toBe(0);
    return result.stdout.trimEnd().split("\n");
  }

  test("CocoaPods is used from OBOP_TOOLS (default ~/build/tools); every cache goes to the host's own folder", () => {
    const [gemHome, path, root, cpHome, cpCache, store, expo, vpHome, lang] = sourced(
      [
        "GEM_HOME",
        "PATH",
        "OBOP_HOST_IOS_ROOT",
        "CP_HOME_DIR",
        "CP_CACHE_DIR",
        "pnpm_config_store_dir",
        "__UNSAFE_EXPO_HOME_DIRECTORY",
        "VP_HOME",
        "LANG",
      ],
      {},
    );
    expect(gemHome).toBe(`${shHome}/build/tools/gems`);
    expect(path!.startsWith(`${shHome}/build/tools/gems/bin:`)).toBe(true);
    expect(root).toBe(`${shHome}/build/obop-host-ios`);
    for (const cache of [cpHome, cpCache, store, expo, vpHome]) {
      expect(cache!.startsWith(`${shHome}/build/obop-host-ios/cache/`), cache).toBe(true);
    }
    expect(lang).toBe("en_US.UTF-8");
    const other = `${shHome}/other`;
    expect(
      sourced(["GEM_HOME", "CP_CACHE_DIR"], { OBOP_TOOLS: "/opt/t", OBOP_HOST_IOS_ROOT: other }),
    ).toEqual(["/opt/t/gems", `${other}/cache/cocoapods`]);
  });

  test("it sets no variable that changes the app: the only OBOP_ variable it exports is OBOP_HOST_IOS_ROOT", () => {
    const result = spawnSync("sh", ["-c", `. "${script.replaceAll("\\", "/")}"; env`], {
      encoding: "utf8",
      env: cleanEnv({ HOME: home }),
    });
    expect(result.status, result.stderr).toBe(0);
    const exported = result.stdout
      .split("\n")
      .map((line) => /^(OBOP_[A-Z0-9_]*)=/.exec(line)?.[1])
      .filter((name): name is string => name !== undefined);
    expect(exported).toEqual(["OBOP_HOST_IOS_ROOT"]);
    const names = [
      "OBOP_ALLOW_CLEARTEXT",
      "OBOP_WEBVIEW_DEBUG",
      "OBOP_DEFAULT_SERVER",
      "OBOP_IOS_BUNDLE_ID",
    ];
    expect(sourced(names, {})).toEqual(names.map(() => "<unset>"));
  });

  test("the folder is private to its owner", () => {
    sourced(["OBOP_HOST_IOS_ROOT"], {});
    if (process.platform !== "win32") {
      expect(statSync(join(home, "build", "obop-host-ios")).mode & 0o777).toBe(0o700);
    }
  });
});

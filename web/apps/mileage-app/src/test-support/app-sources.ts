import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/** Test support: reads the source of the app from disk, for the tests that guard what the screens say. */
export interface SourceFile {
  /** Relative to the app folder, with forward slashes. */
  path: string;
  text: string;
}

const appDir = fileURLToPath(new URL("../../", import.meta.url));

function read(path: string): SourceFile {
  return {
    path: relative(appDir, path).replaceAll("\\", "/"),
    text: readFileSync(path, "utf8"),
  };
}

/** The screens (`src/ui/screens`) and `App.tsx`, which holds the screen "Failed": everything the driver reads. */
export function screenSources(): SourceFile[] {
  const dir = join(appDir, "src/ui/screens");
  const screens = readdirSync(dir)
    .filter((name) => /\.tsx?$/.test(name) && !/\.test\./.test(name))
    .sort()
    .map((name) => read(join(dir, name)));
  return [...screens, read(join(appDir, "App.tsx"))];
}

/** Every source of the app that is not a test, the catalogues or test support: where a key may be used. */
export function appSources(): SourceFile[] {
  const files: SourceFile[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) {
        if (!["node_modules", "i18n", "test-support"].includes(name)) walk(path);
      } else if (/\.tsx?$/.test(name) && !/\.test\./.test(name)) {
        files.push(read(path));
      }
    }
  };
  walk(join(appDir, "src"));
  files.push(read(join(appDir, "App.tsx")));
  return files;
}

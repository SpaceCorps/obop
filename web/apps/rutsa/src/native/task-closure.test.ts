import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vite-plus/test";
import { appDir, closure, importsOf } from "../test-support/imports.ts";

// The location task runs headless: the system starts the JavaScript entry without any screen and
// calls the task, and the task opens the database. What it imports must therefore need no screen:
// not the app, not the buttons and their actions, not the UI kit, not React Native.
//
// After `mileage-app` (`src/native/task-closure.test.ts`).

const FORBIDDEN_FILES = [
  "App.tsx",
  "src/native/actions.ts",
  "src/native/permissions.ts",
  "src/native/recorder.ts",
];
const FORBIDDEN_PACKAGES = [
  "react",
  "react-native",
  "react-native-safe-area-context",
  "@obop/ui-native",
  "expo-status-bar",
  "expo-system-ui",
];

describe("what the headless location task can reach", () => {
  const { files, packages } = closure("src/native/task.ts");

  test("the closure is what the test thinks it is", () => {
    for (const file of [
      "src/native/task.ts",
      "src/native/store.ts",
      "src/native/database.ts",
      "src/native/keychain.ts",
      "src/core/delivery.ts",
      "src/core/open-db.ts",
      "src/core/sql-store.ts",
    ]) {
      expect(files.has(file), file).toBe(true);
    }
    for (const pkg of ["expo-task-manager", "expo-location", "expo-sqlite", "expo-secure-store"]) {
      expect(packages.has(pkg), pkg).toBe(true);
    }
  });

  test("it holds no screen, no action of a button and no UI", () => {
    expect([...files].filter((f) => FORBIDDEN_FILES.includes(f))).toEqual([]);
    expect([...packages].filter((p) => FORBIDDEN_PACKAGES.includes(p))).toEqual([]);
  });

  test("the reader of imports sees what it should", () => {
    expect(
      importsOf(
        [
          'import a from "./a.ts";',
          'import type { B } from "./b.ts";',
          'import {\n  c,\n  type D,\n} from "./c.ts";',
          'import "./side.ts";',
          'export { e } from "./e.ts";',
          'export type { F } from "./f.ts";',
          'const g = await import("./g.ts");',
          '// import h from "./h.ts";',
        ].join("\n"),
      ).sort(),
    ).toEqual(["./a.ts", "./c.ts", "./e.ts", "./g.ts", "./side.ts"]);
  });
});

describe("the entry point", () => {
  test("defines the task before anything else, the app included", () => {
    const imports = importsOf(readFileSync(join(appDir, "index.ts"), "utf8"));
    expect(imports[0]).toBe("./src/native/task.ts");
    expect(imports).toContain("./App.tsx");
  });
});

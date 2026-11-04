import { describe, expect, test } from "vite-plus/test";
import { closure, importsOf } from "../test-support/imports.ts";

// The location task runs headless: the system starts the JavaScript entry without any screen and
// calls the task, and the task opens the outbox. What it imports must therefore not need a screen,
// a sign-in, the browser or the runtime object. The closure of `native/task.ts` may contain
// react-native (the platform is read in `database.ts`) and the database, but not the files below.
// Nor may it reach the instances that an app configures: those are given to the host at start
// (`startHost` in `index.ts`), and the library code reads them when it is called, not when it is
// imported.

const FORBIDDEN_FILES = [
  "src/native/runtime.ts",
  "src/native/auth.ts",
  "src/native/actions.ts",
  "src/native/devices.ts",
  "config/startup.ts",
  "config/identity.ts",
];
const FORBIDDEN_PREFIXES = ["src/ui/"];
const FORBIDDEN_PACKAGES = ["expo-web-browser"];

describe("what the headless location task can reach", () => {
  const { files, packages } = closure("src/native/task.ts");

  test("the closure is what the test thinks it is", () => {
    expect(files.has("src/native/task.ts")).toBe(true);
    expect(files.has("src/native/store.ts")).toBe(true);
    expect(files.has("src/native/database.ts")).toBe(true);
    expect(files.has("src/core/open-outbox.ts")).toBe(true);
    expect(packages.has("expo-task-manager")).toBe(true);
  });

  test("it holds no screen, no sign-in, no runtime and no instance of an app's configuration", () => {
    const offenders = [...files].filter(
      (f) => FORBIDDEN_FILES.includes(f) || FORBIDDEN_PREFIXES.some((p) => f.startsWith(p)),
    );
    expect(offenders).toEqual([]);
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

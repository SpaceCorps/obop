import { fileURLToPath } from "node:url";
import { expect, test } from "vite-plus/test";
import { isMain } from "./main.ts";

test("a script started by its own path is the main module, on every platform", () => {
  // The path Node puts in argv[1] is a platform path (D:\...\build.ts on Windows).
  const url = new URL("../build.ts", import.meta.url).href;
  expect(isMain(url, fileURLToPath(url))).toBe(true);
});

test("an imported module is not the main module", () => {
  const url = new URL("../build.ts", import.meta.url).href;
  expect(isMain(url, fileURLToPath(new URL("../assemble.ts", import.meta.url)))).toBe(false);
  expect(isMain(url, undefined)).toBe(false);
});

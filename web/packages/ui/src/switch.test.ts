import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";

// The native checkbox of a switch covers the switch to take the clicks and is never drawn. The
// rule that dims a disabled checkbox (`input[type="checkbox"]:disabled`) is more specific than
// `.switch input`, so without a rule of its own a disabled switch showed a grey box over its
// label.
const css = readFileSync(new URL("./components.css", import.meta.url), "utf8");

const block = (selector: string): string | undefined => {
  const at = css.indexOf(`\n${selector} {`);
  return at < 0 ? undefined : css.slice(at, css.indexOf("}", at));
};

describe("switch", () => {
  test("a disabled switch keeps its checkbox invisible", () => {
    expect(block(".switch input:disabled")).toMatch(/opacity:\s*0;/);
  });
});

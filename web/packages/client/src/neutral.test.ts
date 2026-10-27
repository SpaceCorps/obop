import { readFileSync, readdirSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";

// The client runs in a browser and in React Native, so the code it ships may use the global
// `fetch`, `Request`, `Response` and `Headers` and nothing that belongs to one platform only.
const dir = new URL(".", import.meta.url);
const shipped = readdirSync(dir).filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"));

const withoutComments = (text: string) =>
  text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");

describe("the runtime code is platform neutral", () => {
  test("there is runtime code to look at", () => {
    expect(shipped).toEqual(
      expect.arrayContaining(["client.ts", "errors.ts", "index.ts", "meta.ts"]),
    );
  });

  test.each(shipped)("%s uses no DOM, Node-only or React Native API", (file) => {
    const code = withoutComments(readFileSync(new URL(file, dir), "utf8"));
    const forbidden = [
      /\bwindow\b/,
      /\bdocument\b/,
      /\bnavigator\b/,
      /\blocalStorage\b/,
      /\bsessionStorage\b/,
      /\bprocess\b/,
      /\bBuffer\b/,
      /\brequire\s*\(/,
      /\b__dirname\b/,
      /from\s+["']node:/,
      /from\s+["'](fs|path|os|http|https|net|crypto|stream|url|util)["']/,
      /from\s+["']react-native["']/,
      /from\s+["']expo/,
      /\bXMLHttpRequest\b/,
    ];
    for (const pattern of forbidden) expect(code, `${file}: ${pattern}`).not.toMatch(pattern);
  });
});

// A person is never shown a plan's id as text: nobody can use eight characters. The places that
// used to print "(plan 01234567)" now say "Sent for approval." and link to the plan (`sent.ts`).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vite-plus/test";

const SRC = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return files(p);
    return /\.(tsx?)$/.test(n) && !/\.test\./.test(n) ? [p] : [];
  });
}

describe("the sentences of the back office", () => {
  const all = files(SRC).map((p) => [p, readFileSync(p, "utf8")] as const);

  it("never cut a plan's id short to print it", () => {
    const bad = all.filter(([, s]) => /planId[^;\n]*\.slice\(/.test(s)).map(([p]) => p);
    expect(bad).toEqual([]);
  });

  it("never put a plan's id into a template string (the full id is no better than eight characters)", () => {
    // The link to a plan (route.ts, sent.ts) and the REST path (api.ts) are the only places that
    // may build text from an id.
    const allowed = /[\\/](api|route|sent)\.ts$/;
    const bad = all
      .filter(([p, s]) => !allowed.test(p) && /\$\{[^}]*[pP]lanId[^}]*\}/.test(s))
      .map(([p]) => p);
    expect(bad).toEqual([]);
  });

  it("have no sentence with a plan in braces", () => {
    const bad = all.filter(([, s]) => /\(plan \{plan\}\)|plan \{plan\}/.test(s)).map(([p]) => p);
    expect(bad).toEqual([]);
  });
});

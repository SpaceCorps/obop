// A person is never shown a plan's id as text: nobody can use eight characters. The places that
// used to print "(plan 01234567)" now say "Sent for approval." and link to the plan (`sent.ts`).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, sep } from "node:path";
import { describe, expect, it } from "vite-plus/test";

const slash = (p: string) => p.split(sep).join("/");
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

  it("say one thing when a change was sent for approval, in the places that had their own words", () => {
    // The connector wizard and catalogue, the two field-service apps and the brand book's specimens
    // said "Approval needed." or "Sent for approval" (no full stop, no link) before; they use
    // `sentText()` / `notifySent` now, and the brand book shows the same toast.
    const own = [
      "connectors/Wizard.tsx",
      "connectors/Catalog.tsx",
      "apps/field_service/JobExtras.tsx",
      "apps/field_service/MyDay.tsx",
      "brand/Components.tsx",
      "brand/Patterns.tsx",
    ];
    const bad = all
      .filter(([p]) => own.some((o) => slash(p).endsWith(`/${o}`)))
      .filter(([, s]) => /Approval needed|["'`]Sent for approval["'`]/.test(s))
      .map(([p]) => p);
    expect(bad).toEqual([]);
    expect(all.filter(([p]) => /connectors\/(Wizard|Catalog)\.tsx$/.test(slash(p)))).toHaveLength(
      2,
    );
    for (const [, s] of all.filter(([p]) => /connectors\/(Wizard|Catalog)\.tsx$/.test(slash(p))))
      expect(s).toMatch(/sentText\(\)/);
  });

  it("have no sentence with a plan in braces", () => {
    const bad = all.filter(([, s]) => /\(plan \{plan\}\)|plan \{plan\}/.test(s)).map(([p]) => p);
    expect(bad).toEqual([]);
  });
});

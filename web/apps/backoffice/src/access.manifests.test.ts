import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vite-plus/test";
import { ADMIN, buildAccess, groupsOfModule, type ModuleLike } from "./access.ts";

// The bundled modules, read from their manifests (packages/*.toml): no server needed. Only what
// access needs is read: each model's name, read group and write group (top-level keys of a
// `[[model]]` table) and the `group = "..."` of an approval step or a guard.

const here = new URL(".", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const PACKAGES = join(here, "../../../../packages");

function parse(text: string): ModuleLike {
  const models: { name: string; read_group?: string; write_group?: string }[] = [];
  const approvals: { group: string }[] = [];
  let id = "";
  let name = "";
  let table = "";
  for (const raw of text.split(/\r?\n/)) {
    const header = /^\s*\[\[?([a-z_.]+)\]\]?\s*$/.exec(raw);
    if (header) {
      table = header[1]!;
      if (table === "model") models.push({ name: "" });
      continue;
    }
    const kv = /^(\w+)\s*=\s*"([^"]*)"\s*$/.exec(raw.trim());
    if (!kv) continue;
    // Keys at the start of the line belong to the table; indented ones belong to a sub-table.
    const top = !/^\s/.test(raw);
    if (table === "module" && kv[1] === "id") id = kv[2]!;
    if (table === "module" && kv[1] === "name") name = kv[2]!;
    if (kv[1] === "group" && kv[2]) approvals.push({ group: kv[2] });
    if (table === "model" && top) {
      const m = models[models.length - 1]!;
      if (kv[1] === "name") m.name = kv[2]!;
      if (kv[1] === "read_group") m.read_group = kv[2]!;
      if (kv[1] === "write_group") m.write_group = kv[2]!;
    }
  }
  return { id, name: name || id, contents: { models, approval_steps: approvals } };
}

const ROWS: ModuleLike[] = readdirSync(PACKAGES)
  .filter((f) => f.endsWith(".toml"))
  .sort()
  .map((f) => parse(readFileSync(join(PACKAGES, f), "utf8")));
const ACCESS = buildAccess(
  ROWS,
  ROWS.map((r) => ({ id: r.id, name: r.name })),
);

// Apps whose ladder is not `<id>.user` / `<id>.manager`, with what the picker offers for each.
// Anything else that differs fails the test below, so a new odd ladder is a deliberate row here.
const ODD: Record<string, { base: string; manage: string | null }> = {
  // The first rung is the agent: Helpdesk has no `helpdesk.user`.
  helpdesk: { base: "helpdesk.agent", manage: "helpdesk.manager" },
};

describe("the bundled manifests", () => {
  it("are all read, hr among them", () => {
    expect(ROWS.length).toBeGreaterThanOrEqual(20);
    expect(ROWS.map((r) => r.id)).toContain("hr");
    for (const r of ROWS) expect(r.id).not.toBe("");
  });

  it("put every group a module names in exactly one place", () => {
    const named = new Set(ROWS.flatMap((r) => groupsOfModule(r)));
    named.delete(ADMIN);
    const placed = new Map<string, number>();
    const put = (g: string | null) => g && placed.set(g, (placed.get(g) ?? 0) + 1);
    for (const l of ACCESS.ladders) {
      put(l.base);
      put(l.manage);
      for (const g of l.extras) put(g);
    }
    for (const g of ACCESS.other) put(g);
    expect([...named].filter((g) => !placed.has(g))).toEqual([]);
    expect([...placed].filter(([, n]) => n > 1)).toEqual([]);
  });

  it("give every app with groups a base, and the ladder <id>.user then <id>.manager unless listed as odd", () => {
    const wrong: string[] = [];
    for (const l of ACCESS.ladders) {
      const want = ODD[l.id] ?? { base: `${l.id}.user`, manage: `${l.id}.manager` };
      if (l.base !== want.base || l.manage !== want.manage)
        wrong.push(
          `${l.id}: base ${l.base}, manage ${l.manage}; expected ${want.base}, ${want.manage}`,
        );
    }
    expect(wrong).toEqual([]);
    // The Swedish chart names the accounting groups only: it has no row of its own.
    const without = ROWS.filter((r) => !ACCESS.ladders.some((l) => l.id === r.id));
    expect(without.map((r) => r.id)).toEqual(["accounting_se"]);
  });
});

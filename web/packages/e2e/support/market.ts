// The tiny marketplace the update specs fill (support/serve.ts, "market"): `offer` puts a version of a
// package on the shelf, `withdraw` takes it off. The app instance reads it as OBOP_MARKETPLACE_URL.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { coreSeries, marketDir } from "./env.ts";

export type Pkg = {
  id: string;
  version: string;
  /** TOML field tables after `title`, e.g. `field("plate")`. */
  fields?: string;
  /** TOML `[[migration]]` tables. */
  migrations?: string;
  changelog?: string;
  /** Module id to the range of it this module supports. */
  deps?: Record<string, string>;
  core?: string;
};

const q = (s: string) => JSON.stringify(s);

/** A `[[model.field]]` table. */
export function field(name: string, extra = ""): string {
  return `\n[[model.field]]\nname = ${q(name)}\ndescription = ${q(name)}\nclass = "none"\n${extra}\n`;
}

/** The obop.toml text of a package: model `<id>_item` with a `title`, plus what the spec adds. */
export function pkgText(p: Pkg): string {
  const deps = Object.entries(p.deps ?? {})
    .map(([id, range]) => `${id} = ${q(range)}`)
    .join("\n");
  return `[module]
id = ${q(p.id)}
version = ${q(p.version)}
name = ${q(p.id)}
description = ${q(`End-to-end package ${p.id}`)}
summary = ${q(`Summary of ${p.id}`)}
author = "E2E"
license = "Apache-2.0"
requires_core = ${q(p.core ?? coreSeries)}
changelog = ${q(p.changelog ?? "")}
${deps ? `\n[module.dependencies]\n${deps}\n` : ""}
[[model]]
name = ${q(`${p.id}_item`)}
description = "An item"
read_group = ${q(`${p.id}.user`)}
write_group = ${q(`${p.id}.user`)}

[[model.field]]
name = "title"
description = "Title"
class = "none"
${p.fields ?? ""}
${p.migrations ?? ""}`;
}

function indexPath() {
  return join(marketDir(), "index.json");
}

type Entry = Record<string, unknown> & { id: string };

function readIndex(): Entry[] {
  try {
    return JSON.parse(readFileSync(indexPath(), "utf8")) as Entry[];
  } catch {
    return [];
  }
}

/** Put `p` on the shelf (replacing any other version of it). */
export function offer(p: Pkg): void {
  mkdirSync(join(marketDir(), "packages"), { recursive: true });
  writeFileSync(join(marketDir(), "packages", `${p.id}.toml`), pkgText(p));
  const entry: Entry = {
    id: p.id,
    name: p.id,
    version: p.version,
    description: `End-to-end package ${p.id}`,
    author: "E2E",
    summary: `Summary of ${p.id}`,
    license: "Apache-2.0",
    requires_core: p.core ?? coreSeries,
    changelog: p.changelog ?? "",
    dependencies: p.deps ?? {},
  };
  writeFileSync(indexPath(), JSON.stringify([...readIndex().filter((e) => e.id !== p.id), entry]));
}

/** Take a package off the shelf. */
export function withdraw(id: string): void {
  rmSync(join(marketDir(), "packages", `${id}.toml`), { force: true });
  writeFileSync(indexPath(), JSON.stringify(readIndex().filter((e) => e.id !== id)));
}

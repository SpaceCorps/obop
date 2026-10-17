// Builds obop.toml manifests for throwaway modules. A test that asserts counts or totals installs its
// own module, so the numbers cannot be disturbed by whatever any other test created.

export type FieldSpec = {
  name: string;
  kind?: "text" | "longtext" | "number" | "money" | "date" | "bool" | "select" | "ref";
  required?: boolean;
  options?: string[];
  to?: string;
  class?: string;
  personal?: boolean;
  description?: string;
};

/** A workflow step: `field := to` when the record is in `from` (any state when omitted). */
export type ActionSpec = {
  name: string;
  field: string;
  from?: string;
  to: string;
  /** Needs a human's approval when an agent runs it. */
  approval?: boolean;
  /** Group that may run it; defaults to the model's write group (`<id>.manager`). */
  group?: string;
};

export type ModuleSpec = {
  /** Lower case letters, digits and underscores. Also the model name. */
  id: string;
  title: string;
  fields: FieldSpec[];
  actions?: ActionSpec[];
};

import { coreSeries } from "./env.ts";

const q = (s: string) => JSON.stringify(s);

export function manifest(m: ModuleSpec): string {
  const lines = [
    "[module]",
    `id = ${q(m.id)}`,
    'version = "0.1.0"',
    `name = ${q(m.title)}`,
    `description = ${q(`${m.title} (end-to-end test module)`)}`,
    'author = "obop e2e"',
    `requires_core = ${q(coreSeries)}`,
    "",
    "[[model]]",
    `name = ${q(m.id)}`,
    `description = ${q(m.title)}`,
    `read_group = ${q(`${m.id}.user`)}`,
    `write_group = ${q(`${m.id}.manager`)}`,
  ];
  for (const f of m.fields) {
    lines.push(
      "",
      "  [[model.field]]",
      `  name = ${q(f.name)}`,
      `  description = ${q(f.description ?? f.name)}`,
      `  class = ${q(f.class ?? "none")}`,
    );
    if (f.personal) lines.push("  personal_data = true");
    if (f.required) lines.push("  required = true");
    if (f.kind && f.kind !== "text") lines.push(`  kind = ${q(f.kind)}`);
    if (f.kind === "select") lines.push(`  options = [${(f.options ?? []).map(q).join(", ")}]`);
    if (f.kind === "ref") lines.push(`  to = ${q(f.to ?? "")}`);
  }
  for (const a of m.actions ?? []) {
    lines.push(
      "",
      "  [[model.action]]",
      `  name = ${q(a.name)}`,
      `  description = ${q(`${a.name} (end-to-end test action)`)}`,
      `  field = ${q(a.field)}`,
    );
    if (a.from) lines.push(`  from = ${q(a.from)}`);
    lines.push(`  to = ${q(a.to)}`);
    if (a.group) lines.push(`  group = ${q(a.group)}`);
    if (a.approval) lines.push("  approval = true");
  }
  return `${lines.join("\n")}\n`;
}

import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";

/**
 * The age bound of scheduled backups is a figure an operator may copy into a privacy notice. Unset,
 * it follows the counts (`retention::default_max_age_days`: 31 days per monthly, 7 per weekly and 1
 * per daily backup kept, plus 28 days), so 400 days holds for the default policy only. The reference
 * pages must not state 400 days without that condition.
 */
const page = (name: string) =>
  readFileSync(new URL(`../../content/en/docs/reference/${name}`, import.meta.url), "utf8");

describe("the reference pages state the backup age bound with its condition", () => {
  for (const name of ["environment.md", "cli.md"]) {
    test(name, () => {
      const lines = page(name)
        .split("\n")
        .filter((l) => l.includes("400 days"));
      expect(lines.length).toBeGreaterThan(0);
      for (const l of lines) expect(l).toContain("plus 28 days");
    });
  }
});

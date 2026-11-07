import { existsSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The folder of the app, where `src` and `config` are. */
export const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/**
 * Imports of a source text. Type-only imports and exports are erased when the app is built, so
 * they are left out unless `includeTypes` asks for them (a boundary between two parts of the code
 * is about both).
 */
export function importsOf(source: string, includeTypes = false): string[] {
  const found: string[] = [];
  const statement =
    /(?:^|\n)\s*(import|export)\s+(type\s+)?(?:[^"';]*?\s+from\s+)?["']([^"']+)["']/g;
  for (const match of source.matchAll(statement)) {
    if (match[2] === undefined || includeTypes) found.push(match[3]!);
  }
  for (const match of source.matchAll(/\bimport\(\s*["']([^"']+)["']\s*\)/g)) found.push(match[1]!);
  return found;
}

/** The files a file imports directly (paths relative to the app, with `/`) and the packages it names. */
export function directImports(
  entry: string,
  includeTypes = false,
): { files: string[]; packages: string[] } {
  const file = resolve(appDir, entry);
  const files: string[] = [];
  const packages: string[] = [];
  for (const spec of importsOf(readFileSync(file, "utf8"), includeTypes)) {
    if (!spec.startsWith(".")) {
      packages.push(spec);
      continue;
    }
    const target = resolve(dirname(file), spec);
    const next = [target, `${target}.ts`, `${target}.tsx`, join(target, "index.ts")].find(
      (candidate) => existsSync(candidate) && /\.tsx?$/.test(candidate),
    );
    if (next === undefined) throw new Error(`${entry} imports ${spec}, which is not a file`);
    files.push(relative(appDir, next).replaceAll("\\", "/"));
  }
  return { files, packages };
}

/** Everything a file imports, directly or not, at run time. */
export function closure(entry: string): { files: Set<string>; packages: Set<string> } {
  const files = new Set<string>();
  const packages = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const next = queue.pop()!;
    if (files.has(next)) continue;
    files.add(next);
    const direct = directImports(next);
    for (const p of direct.packages) packages.add(p);
    queue.push(...direct.files);
  }
  return { files, packages };
}

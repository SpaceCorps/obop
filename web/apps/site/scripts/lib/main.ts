import { pathToFileURL } from "node:url";

/**
 * Whether the module whose `import.meta.url` is `url` is the script Node was started with
 * (`node scripts/build.ts`). The path in `argv[1]` is turned into a file URL first: on Windows it
 * is `D:\...`, which `file://` plus the path never matches (docs/ISSUES.md U47).
 */
export function isMain(url: string, script: string | undefined = process.argv[1]): boolean {
  return script !== undefined && url === pathToFileURL(script).href;
}

// Whether a script is the one Node was started with, and not just imported by another script.
// The old test compared the module URL with `file:///` plus the path, which holds only for a
// Windows path without spaces: on macOS and Linux the path starts with a slash and the URL had
// four, and a space is `%20` in the one and a space in the other. Both sides are turned into real
// paths (links followed) and compared as paths.
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

/**
 * @param {string} moduleUrl `import.meta.url` of the script that asks
 * @param {string | undefined} entry `process.argv[1]`
 */
export function isMain(moduleUrl, entry) {
  if (!entry) return false;
  try {
    return realpathSync(fileURLToPath(moduleUrl)) === realpathSync(entry);
  } catch {
    return false;
  }
}

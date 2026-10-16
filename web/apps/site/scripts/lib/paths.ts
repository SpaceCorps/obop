import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const SITE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const REPO_ROOT = resolve(SITE_ROOT, "../../..");
/** The assembled VitePress source tree (generated, not committed). */
export const SRC = resolve(SITE_ROOT, ".src");
/** Generated data the theme imports (apps, docs navigation, page manifest). */
export const GEN = resolve(SITE_ROOT, ".generated");
export const DIST = resolve(SITE_ROOT, "dist");

/** Public origin used for canonical URLs, the sitemap and social cards. */
export const SITE_URL = (
  process.env["OBOP_SITE_URL"] ?? "https://obop-platform.sliplane.app"
).replace(/\/$/, "");
export const GITHUB = "https://github.com/SpaceCorps/obop";

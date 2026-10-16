// Builds the whole site: assemble the sources, run VitePress, then the post-build step (sitemap,
// robots, llms.txt, search indexes, social images, 404 and so on). `vp run build` or `vp build`.
import { rmSync } from "node:fs";
import { build } from "vitepress";
import { assemble } from "./assemble.ts";
import { postbuild } from "./postbuild.ts";
import { DIST, SITE_ROOT } from "./lib/paths.ts";

export async function buildSite(): Promise<void> {
  const t0 = Date.now();
  const records = assemble();
  console.log(`assembled ${records.length} pages`);
  rmSync(DIST, { recursive: true, force: true });
  await build(SITE_ROOT, {});
  await postbuild(records);
  console.log(`site built in ${((Date.now() - t0) / 1000).toFixed(1)}s -> ${DIST}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await buildSite();
}

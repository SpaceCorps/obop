import { defineConfig } from "../../config/index.ts";

/**
 * The website is built by VitePress, which is its own CLI on top of Vite. So that `vp build` (the one
 * command CI and the Dockerfile run for every app) builds the whole site, this plugin gives Vite an
 * empty virtual entry and runs the real pipeline (assemble the sources, VitePress, post-build) when
 * that trivial build finishes. Nothing is written by the empty build itself.
 */
const ENTRY = "virtual:obop-site";

function site() {
  return {
    name: "obop-site",
    apply: "build" as const,
    config: () => ({
      logLevel: "warn" as const,
      build: { write: false, rollupOptions: { input: ENTRY } },
    }),
    resolveId: (id: string) => (id === ENTRY ? `\0${ENTRY}` : null),
    load: (id: string) => (id === `\0${ENTRY}` ? "export default 0;" : null),
    async closeBundle() {
      const { buildSite } = await import("./scripts/build.ts");
      await buildSite();
    },
  };
}

export default defineConfig({ plugins: [site()] });

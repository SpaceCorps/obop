import { fileURLToPath } from "node:url";
import { defineConfig } from "../../config/index.ts";

// The module guide imports docs/MODULES.md from the repository root as raw text, so the dev
// server may serve files from there. The build bundles it as a string.
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

export default defineConfig({
  server: { fs: { allow: [repoRoot] } },
});

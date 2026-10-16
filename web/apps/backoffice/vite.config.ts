import { fileURLToPath } from "node:url";
import { defineConfig } from "../../config/index.ts";

// The module guide imports docs/MODULES.md from the repository root as raw text, so the dev
// server may serve files from there. The build bundles it as a string.
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

// Under a public site the back office is served from /admin/ (set OBOP_ADMIN_BASE=/admin/).
export default defineConfig({
  base: process.env.OBOP_ADMIN_BASE ?? "/",
  server: { fs: { allow: [repoRoot] } },
});

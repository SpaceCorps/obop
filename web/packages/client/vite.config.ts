import { defineConfig } from "../../config/index.ts";

// The package is consumed as TypeScript source by the apps (Vite for the web, Metro for the
// Mileage app); this build is the library check (types, bundling) that `vp build` runs for it.
// `vp check` only reports type errors when lint.options.typeCheck is on (it needs typeAware).
// `src/generated/schema.d.ts` is written by `vp run generate:types`; it is not formatted or linted
// by hand, and CI regenerates it and fails on a difference.
export default defineConfig({
  lint: {
    options: { typeAware: true, typeCheck: true },
    ignorePatterns: ["src/generated/**"],
  },
  fmt: { ignorePatterns: ["src/generated/**", "openapi.json"] },
  build: {
    lib: { entry: "src/index.ts", formats: ["es"], fileName: "index" },
    rollupOptions: { external: ["openapi-fetch"] },
  },
});

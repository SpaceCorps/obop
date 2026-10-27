import { defineConfig } from "../../config/index.ts";

// The package is consumed as TypeScript source by the apps (Vite for the web, Metro for the
// Mileage app); this build is the library check (types, bundling) that `vp build` runs for it.
// `vp check` only reports type errors when lint.options.typeCheck is on (it needs typeAware).
export default defineConfig({
  lint: { options: { typeAware: true, typeCheck: true } },
  build: {
    lib: { entry: "src/index.ts", formats: ["es"], fileName: "index" },
  },
});

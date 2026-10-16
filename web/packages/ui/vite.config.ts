import { defineConfig } from "../../config/index.ts";

// The package is consumed as source by the apps; this build is the library check (types, bundling,
// CSS resolution) that `vp build` runs for it.
export default defineConfig({
  build: {
    lib: { entry: "src/index.ts", formats: ["es"], fileName: "index" },
    rollupOptions: { external: ["react", "react-dom", "react/jsx-runtime"] },
  },
});

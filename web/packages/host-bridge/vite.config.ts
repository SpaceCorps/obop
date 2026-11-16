import { defineConfig } from "../../config/index.ts";

// The package is consumed as TypeScript source: by the host app (Metro), by the back office (Vite)
// and by the stand-in host of the end-to-end tests (Node). Nothing is built from it. `vp check`
// reports type errors only when lint.options.typeCheck is on (it needs typeAware).
export default defineConfig({
  lint: { options: { typeAware: true, typeCheck: true } },
});

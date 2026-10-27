import { defineConfig } from "../../config/index.ts";

// Metro, the Expo CLI and Gradle build the app (the mobile-toolchain exception, OR-34); Vite+ only
// checks the TypeScript here. `vp check` reports type errors when lint.options.typeCheck is on.
export default defineConfig({
  lint: { options: { typeAware: true, typeCheck: true } },
});

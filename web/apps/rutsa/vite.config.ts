import { defineConfig } from "../../config/index.ts";

// The Expo CLI, Metro and Gradle build the app (the mobile-toolchain exception, OR-34 b). Vite+
// only checks the TypeScript here and runs the tests: `vp check` reports type errors because
// lint.options.typeCheck is on.
export default defineConfig({
  lint: { options: { typeAware: true, typeCheck: true } },
});

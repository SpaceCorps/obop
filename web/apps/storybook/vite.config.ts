import { defineConfig } from "../../config/index.ts";

// The web Storybook is built and run by Storybook's own CLI through `vp run storybook` and
// `vp run build-storybook` (it brings its own Vite). Vite+ only checks the TypeScript here.
export default defineConfig({
  lint: { options: { typeAware: true, typeCheck: true }, ignorePatterns: ["storybook-static/**"] },
  fmt: { ignorePatterns: ["storybook-static/**"] },
});

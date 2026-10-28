import { createRequire } from "node:module";
import { dirname } from "node:path";
import type { StorybookConfig } from "@storybook/react-vite";

// Vite wants an absolute target for an alias: the folder of react-native-web.
const reactNativeWeb = dirname(
  createRequire(import.meta.url).resolve("react-native-web/package.json"),
);

// The stories of the native components live here (`src/`), so that the component package needs no
// Storybook dependency; the stories of the web packages sit next to their components.
const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx)", "../../../packages/*/src/**/*.stories.@(ts|tsx)"],
  framework: "@storybook/react-vite",
  core: { disableTelemetry: true },
  addons: [],
  // react-docgen would read the Flow source of react-native to document props; the stories do not need it.
  typescript: { reactDocgen: false },
  viteFinal(config) {
    return {
      ...config,
      resolve: {
        ...config.resolve,
        // The React Native components import `react-native`; the browser draws them with
        // react-native-web. One copy of React serves the stories and the components.
        alias: {
          ...(config.resolve?.alias as Record<string, string> | undefined),
          "react-native": reactNativeWeb,
        },
        dedupe: ["react", "react-dom"],
      },
      define: { ...config.define, __DEV__: JSON.stringify(true) },
    };
  },
};

export default config;

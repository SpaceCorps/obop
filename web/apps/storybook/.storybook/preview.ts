import type { Preview } from "@storybook/react-vite";

const preview: Preview = {
  parameters: {
    layout: "fullscreen",
    // The stories draw their own page background (the theme), light or dark.
    backgrounds: { disabled: true },
  },
};

export default preview;

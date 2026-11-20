// A story for `InstanceLogo` in Component Story Format 3, written with no dependency on Storybook:
// the web Storybook (`web/apps/storybook`, OR-34 g) picks it up through its story glob over
// `web/packages/*/src` and renders it; CI builds it with `vp run build-storybook`. `vitest` does not
// collect it; the `InstanceLogo` tests cover the component.
//
// The artwork is an `<img>` of the logo route (`/api/brand/logo.svg`), which a Storybook has no
// server for: the stories show the frame, the name and the sizes, and the image as a broken one.
// The design system, so the story is drawn with the classes the apps load.
import "./styles.css";
import { InstanceLogo, type InstanceLogoProps } from "./instance-logo.tsx";
import type { PageBrand } from "./brand.ts";

const brand: PageBrand = {
  name: "MOHV",
  slug: "mohv",
  messages: {},
  v: "0123456789abcdef",
};

export default {
  title: "Brand/InstanceLogo",
  component: InstanceLogo,
  args: { brand, variant: "full", size: 28 } satisfies InstanceLogoProps,
};

type Story = { args?: InstanceLogoProps; render?: (args: InstanceLogoProps) => React.ReactNode };

/** The artwork and the name, as on the sign-in screens. */
export const Full: Story = { args: { variant: "full", size: 28 } };

/** The artwork alone, with the name for a screen reader, as in the top bar. */
export const Mark: Story = { args: { variant: "mark", size: 22 } };

/** A long name wraps inside its box instead of pushing the page sideways. */
export const LongName: Story = {
  args: { brand: { ...brand, name: "The Very Long Name of a Company That Wraps" }, size: 28 },
};

/** No brand on the page: obop's own mark, as before. */
export const NoBrand: Story = { args: { brand: null, variant: "full", size: 28 } };

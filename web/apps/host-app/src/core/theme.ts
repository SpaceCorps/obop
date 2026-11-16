// The ground behind the page and the style of the status bar. They follow what the page reports
// with `host.theme`; before the first report, the system's scheme and the ground of the tokens.
import { isHexColour, type Scheme } from "@obop/host-bridge";
import { tokens } from "@obop/ui-native/tokens";

export { isHexColour };
export type { Scheme };

/** What the page said about itself: its ground colour as `#rrggbb` and whether it is a dark one. */
export interface PageTheme {
  readonly scheme: Scheme;
  readonly background: string;
}

/** The system's scheme as the app reads it: anything but `dark` is light. */
export function schemeOfSystem(system: string | null | undefined): Scheme {
  return system === "dark" ? "dark" : "light";
}

/** The ground before the page has reported one: the page's own token for it. */
export function startGround(systemScheme: Scheme): string {
  return tokens[systemScheme].bg;
}

/** The ground the app paints behind the page and under the system bars. */
export function groundOf(theme: PageTheme | null, systemScheme: Scheme): string {
  return theme !== null && isHexColour(theme.background)
    ? theme.background
    : startGround(systemScheme);
}

/** The style of the status bar's icons and text on a ground of `scheme`: light ones on a dark ground. */
export function barStyle(scheme: Scheme): "light" | "dark" {
  return scheme === "dark" ? "light" : "dark";
}

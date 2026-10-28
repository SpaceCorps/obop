import type { Purpose } from "../core/types.ts";

/**
 * The screens, as data. The app has six, so the state is a small stack in `App.tsx` and not a
 * navigation library (docs/spec/mobile.md 8.7): fewer native modules, fewer versions to pin.
 */
export type Route =
  | { name: "home" }
  | { name: "trips" }
  | { name: "trip"; id: string }
  | { name: "account" }
  | { name: "recording" }
  | { name: "permissions"; purpose: Purpose };

export interface Nav {
  push: (route: Route) => void;
  /** Replaces the current screen. */
  replace: (route: Route) => void;
  /** Goes back one screen; does nothing on the first. */
  back: () => void;
  /** Back to the home screen, whatever is on the stack. */
  home: () => void;
}

/** The stack after a push, a replace, a back or a home. Pure, so it is tested. */
export function reduce(
  stack: readonly Route[],
  action: { type: "push" | "replace"; route: Route } | { type: "back" | "home" },
): Route[] {
  switch (action.type) {
    case "push":
      return [...stack, action.route];
    case "replace":
      return [...stack.slice(0, -1), action.route];
    case "back":
      return stack.length > 1 ? stack.slice(0, -1) : [...stack];
    case "home":
      return [{ name: "home" }];
  }
}

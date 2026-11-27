/**
 * The screens of the stack, as data. The app has four routes, so the state is a small stack in
 * `App.tsx` and not a navigation library: fewer native modules, fewer versions to pin. At the
 * bottom of the stack (`home`) are the three tabs of the switcher (`Tab`): History, Profile and
 * Record, the home screen where a trip is recorded (D4); a trip's screen, Manual add and the
 * permissions are pushed over them, without the switcher.
 *
 * Location is a route and never a gate (BR-36, US-18): without location access the trips and the
 * trips added by hand stay reachable; only what needs a position goes to `permissions`, and Close
 * there goes back to where the driver was.
 *
 * After `mileage-app` (`src/ui/router.ts`), without its gates of the sign-in.
 */
export type Route =
  | { name: "home" }
  /**
   * Why location is needed and what to do about it (D2). `next` is what the driver was doing:
   * when they come back from the settings with location allowed, it goes on without another tap
   * (US-18 AC5): `start` a new trip, `resume` the paused one, or `restart` the recording of the
   * open trip after the check of C5 found that the phone could not record. (Not called `then`: an
   * object with `then` is taken for a promise by `await`.)
   */
  | { name: "permissions"; next: "start" | "resume" | "restart" }
  | { name: "trip"; id: string }
  /** A trip added by hand (D7). */
  | { name: "add" };

export interface Nav {
  push: (route: Route) => void;
  /** Replaces the current screen. */
  replace: (route: Route) => void;
  /** Goes back one screen; does nothing on the first. */
  back: () => void;
  /** Back to the home screen, whatever is on the stack. */
  home: () => void;
  /** Shows a tab of the switcher, over nothing (the stack is emptied to the tabs). */
  tab: (tab: Tab) => void;
}

/** The tabs of the switcher at the bottom, in its order; Record, in the middle, is the home screen. */
export type Tab = "history" | "record" | "profile";
export const TABS: readonly Tab[] = ["history", "record", "profile"];

export type NavAction = { type: "push" | "replace"; route: Route } | { type: "back" | "home" };

const HOME: Route = { name: "home" };

/** The stack after a push, a replace, a back or a home. Pure, so it is tested. */
export function reduce(stack: readonly Route[], action: NavAction): Route[] {
  switch (action.type) {
    case "push":
      return [...stack, action.route];
    case "replace":
      return stack.length === 0 ? [action.route] : [...stack.slice(0, -1), action.route];
    case "back":
      return stack.length > 1 ? stack.slice(0, -1) : [...stack];
    case "home":
      return [HOME];
  }
}

/** The screen on top of the stack; home when the stack is empty. */
export function current(stack: readonly Route[]): Route {
  return stack[stack.length - 1] ?? HOME;
}

/**
 * The system back button of Android: one screen back while there is one; on the first screen the
 * press is left to the system, which leaves the app.
 */
export function canGoBack(stack: readonly Route[]): boolean {
  return stack.length > 1;
}

/**
 * What the back button of Android does: one screen back while one is pushed over the tabs; on
 * Profile or History it goes to Record; on Record the press is left to the system, which leaves
 * the app.
 */
export function backPress(stack: readonly Route[], tab: Tab): "back" | "record" | "leave" {
  if (canGoBack(stack)) return "back";
  return tab === "record" ? "leave" : "record";
}

/**
 * The screens of the stack, as data. The app has six routes, so the state is a small stack in
 * `App.tsx` and not a navigation library: fewer native modules, fewer versions to pin.
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
   * (US-18 AC5). (Not called `then`: an object with `then` is taken for a promise by `await`.)
   */
  | { name: "permissions"; next: "start" }
  | { name: "recording" }
  | { name: "trips" }
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
}

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
 * The stack once the trips have been read at the start: a trip that is open (recording or paused)
 * is shown first, on the screen of the recording, where Pause and Stop are (D4).
 */
export function openTripFirst(stack: readonly Route[], openTrip: boolean): Route[] {
  if (!openTrip || current(stack).name === "recording") return [...stack];
  return reduce(stack, { type: "push", route: { name: "recording" } });
}

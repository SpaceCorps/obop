import type { SessionStatus } from "../core/session.ts";
import type { Purpose } from "../core/types.ts";

/**
 * The screens of the stack, as data. The app has six routes (and two gates before them, Sign-in
 * and the trips of another sign-in, see `gate`), so the state is a small stack in `App.tsx` and not a
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

/** Which part of the app the driver sees before any route is looked at. */
export type Gate =
  /** The outbox is being read. */
  | "loading"
  /** Nobody is signed in and a trip is open: the recording, with Stop, comes first. */
  | "recording"
  | "sign_in"
  /** The phone holds trips of another sign-in: only the choice about them is shown. */
  | "foreign"
  | "app";

/**
 * Whether the driver's choice to go on to the sign-in from the recording still stands. It ends
 * when a sign-in has finished: the next time the sign-in ends during a trip, the recording comes
 * first again, not the sign-in screen.
 */
export const keepSigningIn = (signingIn: boolean, status: SessionStatus): boolean =>
  signingIn && status !== "signed_in";

/**
 * Decided before the routes, and pure so it is tested. The rule that matters: a trip that is open
 * is never hidden behind the sign-in screen, because the Stop button is on the recording screen
 * and stopping needs no connection and no sign-in.
 */
export function gate(input: {
  status: SessionStatus;
  /** The outbox holds trips of another sign-in. */
  foreign: boolean;
  openTrip: boolean;
  /** The outbox has been read at least once. */
  loaded: boolean;
  /** The driver chose to sign in from the recording screen. */
  signingIn: boolean;
}): Gate {
  if (input.status !== "signed_in") {
    if (!input.loaded) return "loading";
    return input.openTrip && !input.signingIn ? "recording" : "sign_in";
  }
  return input.foreign ? "foreign" : "app";
}

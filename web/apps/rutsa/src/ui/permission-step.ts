import type { CannotRecord } from "../core/recording.ts";
import type { LocationAccess } from "../native/permissions.ts";

/**
 * The steps of the way to a recording (`screens/Permissions.tsx`), as data, so the order is tested.
 *
 * After `mileage-app` (`src/ui/permission-step.ts`), with what US-18 adds: a return from the
 * settings checks again and goes on by itself (`stepOnReturn`).
 */
export type Step =
  /** The settings and the access are being read. */
  | "check"
  /** Why location is needed and where the route stays (BR-20); its button shows the system dialog. */
  | "disclosure"
  /** Refused once: the dialog can be shown again. */
  | "denied"
  /** Refused for good: only the settings can change it. */
  | "blocked"
  /** Only an approximate location: a trip cannot be measured with it. */
  | "approximate"
  /** Location is switched off for the whole phone. */
  | "services"
  /** Ready, the first time: keep the app running, then Start the trip. */
  | "guide";

/** The step that follows an answer about the location permission (after the system dialog). */
export function stepAfterAccess(access: LocationAccess): Step {
  if (access === "granted") return "guide";
  if (access === "approximate") return "approximate";
  if (access === "services_off") return "services";
  return access === "ask" ? "denied" : "blocked";
}

/**
 * The first step of a Start: the disclosure until it has been read; after that a granted
 * permission goes straight to the trip (`begin`), and whatever stands in the way is shown with its
 * way on. A permission that was not asked yet shows the disclosure again, whose button shows the
 * system dialog.
 */
export function firstStep(accepted: boolean, access: LocationAccess): Step | "begin" {
  if (!accepted) return "disclosure";
  if (access === "granted") return "begin";
  if (access === "ask") return "disclosure";
  return stepAfterAccess(access);
}

/**
 * The step after the phone refused to start (`CannotRecord`). On Android the access reads granted
 * with location switched off, so only the start finds that; a missing permission is shown as the
 * access now says it (refused once, or for good).
 */
export function stepAfterRefusal(reason: CannotRecord["reason"], access: LocationAccess): Step {
  if (reason === "services_off") return "services";
  if (reason === "approximate") return "approximate";
  return stepAfterAccess(access);
}

/**
 * The notifications of Android 13 before a start: the dialog is shown once in the life of the app,
 * and only after a tap of the driver (Continue, Start, Start the trip), never on a start that goes
 * on by itself after a return from the settings, where the driver went for the location. Otherwise
 * the app only reads whether they are allowed, for the note on the screen.
 */
export function notificationsStep(asked: boolean, byTap: boolean): "ask" | "read" {
  return !asked && byTap ? "ask" : "read";
}

/** The steps that send the driver away to fix something; a return to the app checks again. */
export function waitsForFix(step: Step): boolean {
  return step === "denied" || step === "blocked" || step === "approximate" || step === "services";
}

/**
 * The step when the driver comes back to the app while it waits for a fix: with location allowed
 * the trip starts without another tap (US-18 AC5); the start itself finds a location switch that
 * is still off. Otherwise the step shows what still stands in the way.
 */
export function stepOnReturn(access: LocationAccess): Step | "begin" {
  return access === "granted" ? "begin" : stepAfterAccess(access);
}

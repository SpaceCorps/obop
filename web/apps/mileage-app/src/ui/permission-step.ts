import type { LocationAccess } from "../native/permissions.ts";

export type Step =
  | "check"
  | "disclosure"
  | "denied"
  | "blocked"
  | "approximate"
  | "services"
  | "guide";

/** The screen that follows an answer about the location permission. */
export function stepAfterAccess(access: LocationAccess): Step {
  if (access === "granted") return "guide";
  if (access === "approximate") return "approximate";
  if (access === "services_off") return "services";
  return access === "ask" ? "denied" : "blocked";
}

/**
 * The first screen of a Start: the disclosure comes first until it has been read; after that a
 * granted permission goes straight to the trip (`begin`), and whatever stands in the way is shown
 * with its way on. A permission that was not asked yet, or was refused once, shows the disclosure
 * again, whose button shows the system dialog.
 */
export function firstStep(accepted: boolean, access: LocationAccess): Step | "begin" {
  if (!accepted) return "disclosure";
  if (access === "granted") return "begin";
  if (access === "ask") return "disclosure";
  return stepAfterAccess(access);
}

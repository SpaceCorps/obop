import type { Readiness } from "../core/liveness.ts";
import type { MessageKey } from "../i18n/index.ts";

/**
 * What the status banner at the top of the screens says (`StatusBanner.tsx`): what keeps the phone
 * from recording a trip well, before the driver presses Start (BR-36), and the notifications of
 * Android 13 that are off, without which a trip records but is not seen (BR-14). Pure, so the
 * cases are tested.
 */
export type Problem = "location_off" | "no_permission" | "approximate" | "notifications_off";

/** What the phone says now: the readiness of the location, and whether notifications are allowed. */
export interface PhoneStatus {
  readiness: Readiness;
  notifications: boolean;
}

/** Each problem once, the location first; nothing when the phone can record and show it. */
export function problemsOf(status: PhoneStatus): Problem[] {
  const problems: Problem[] = [];
  if (status.readiness === "services_off") problems.push("location_off");
  else if (status.readiness === "no_permission") problems.push("no_permission");
  else if (status.readiness === "approximate") problems.push("approximate");
  if (!status.notifications) problems.push("notifications_off");
  return problems;
}

/**
 * Whether the banner shows: a problem the driver has not closed in this start of the app. A
 * problem that comes later (location switched off after the notifications were closed) shows it
 * again; one that went away leaves the others closed.
 */
export function bannerShown(problems: readonly Problem[], closed: ReadonlySet<Problem>): boolean {
  return problems.some((p) => !closed.has(p));
}

/**
 * Which page Open settings opens: the location settings of the phone while location is off there
 * (the most important to fix; the notifications are on the app's page, for the next time), else
 * the app's own page (its permissions and its notifications).
 */
export function settingsFor(problems: readonly Problem[]): "location" | "app" {
  return problems.includes("location_off") ? "location" : "app";
}

type LocationProblem = Exclude<Problem, "notifications_off">;

/** The sentence of a location problem alone. */
const ALONE: Readonly<Record<LocationProblem, MessageKey>> = {
  location_off: "status.location_off",
  no_permission: "status.no_permission",
  approximate: "status.approximate",
};

/** The location problem with the notifications off as well: one sentence for both. */
const WITH_NOTIFICATIONS: Readonly<Record<LocationProblem, MessageKey>> = {
  location_off: "status.location_off.notifications",
  no_permission: "status.no_permission.notifications",
  approximate: "status.approximate.notifications",
};

const isLocation = (p: Problem): p is LocationProblem => p !== "notifications_off";

/**
 * The one short sentence of the banner: what is off, as an ordinary sentence ("Location is
 * off.", "Notifications are off.", "Location and notifications are off."). Each case has its own
 * sentence in the catalogue, so another language words it its own way. `null` without a problem.
 */
export function bannerText(problems: readonly Problem[]): MessageKey | null {
  const location = problems.find(isLocation);
  const notifications = problems.includes("notifications_off");
  if (location !== undefined) return (notifications ? WITH_NOTIFICATIONS : ALONE)[location];
  return notifications ? "status.notifications_off" : null;
}

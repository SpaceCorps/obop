import type { Trip } from "../core/types.ts";
import { DISCLOSURE_ACCEPTED, NOTIFICATIONS_ASKED } from "../core/settings.ts";
import { firstStep, notificationsStep } from "../ui/permission-step.ts";
import { CannotRecord, startTrip } from "./actions.ts";
import { locationAccess, notificationsAllowed, requestNotifications } from "./permissions.ts";
import { getSettings } from "./store.ts";

/** Runs a system dialog; the screen of the permissions marks it, so its return is not a return from the settings. */
export type Ask = <T>(question: () => Promise<T>) => Promise<T>;

/**
 * Whether the notification of a recording can be shown. The dialog of Android 13 comes once in
 * the life of the app, after a tap (`notificationsStep`); otherwise the answer is only read. Shared
 * by the home screen and the screen of the permissions.
 */
export async function notificationsAtStart(
  byTap: boolean,
  ask: Ask = (question) => question(),
): Promise<boolean> {
  const settings = await getSettings();
  const asked = (await settings.get(NOTIFICATIONS_ASKED)) !== null;
  if (notificationsStep(asked, byTap) === "read") return notificationsAllowed();
  await settings.set(NOTIFICATIONS_ASKED, new Date().toISOString());
  return ask(requestNotifications);
}

/**
 * Start from the home screen: when the explanation was read and location is allowed, the trip
 * starts here, with no screen in between (the screen of the permissions would show for a moment
 * while it checks). `null` when it cannot: the explanation is not read yet, location is not
 * allowed, or the phone refused to record (location switched off); the screen of the permissions
 * then shows what stands in the way. Other failures are thrown.
 */
export async function startWhenReady(): Promise<Trip | null> {
  const accepted = (await (await getSettings()).get(DISCLOSURE_ACCEPTED)) !== null;
  if (firstStep(accepted, await locationAccess()) !== "begin") return null;
  // The notifications are only read here, or asked once by this tap; the status banner says
  // when they are off.
  await notificationsAtStart(true).catch(() => true);
  try {
    return await startTrip();
  } catch (e) {
    if (e instanceof CannotRecord) return null;
    throw e;
  }
}

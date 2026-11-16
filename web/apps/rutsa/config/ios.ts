// The pure parts of the iOS configuration (`app.config.ts` applies them), kept here so that
// `vp test` can run them without Expo.

/** Reverse domain form: letters, digits, hyphens and dots, at least two parts (Apple's rule for a bundle id). */
const BUNDLE_ID = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

/**
 * The bundle id of the iOS build: `OBOP_IOS_BUNDLE_ID` when set, else `fallback` (the application
 * id). A test build signed with a free Apple ID takes another id (for example
 * `org.obop.rutsa.test`): an App ID first signed in a free personal team can later not be
 * registered by the SpaceCorps team (Q37 of docs/DECISIONS.md), as found with the Mileage app. The
 * value goes into the Xcode project, so it must have the right form.
 */
export function iosBundleId(env: string | undefined, fallback: string): string {
  if (env === undefined || env === "") return fallback;
  if (!BUNDLE_ID.test(env)) {
    throw new Error(
      `OBOP_IOS_BUNDLE_ID must be a bundle id in reverse domain form (letters, digits, hyphens and dots), got ${JSON.stringify(env)}`,
    );
  }
  return env;
}

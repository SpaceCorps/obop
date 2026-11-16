/**
 * The identity of the build, in one place: `app.config.ts` reads it for the Android package, the
 * iOS bundle id and the version.
 *
 * PLACEHOLDER: "org.obop.rutsa" is a stand-in id, like "org.obop.mileage" of the Mileage app. It is
 * not registered anywhere, and no store account, signing key or publication exists (Q37 of
 * docs/DECISIONS.md). Once an app is on a store its id cannot change, so the owner rules first.
 *
 * Node reads this file when the app is built, so it imports nothing.
 */
export const identity = {
  applicationId: "org.obop.rutsa",
  version: "0.1.0",
} as const;

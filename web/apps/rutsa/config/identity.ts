/**
 * The identity of the build, in one place: `app.config.ts` reads it for the Android package, the
 * iOS bundle id and the version; the app reads the storage names (`src/native/database.ts`).
 *
 * PLACEHOLDER: "org.obop.rutsa" is a stand-in id, like "org.obop.mileage" of the Mileage app. It is
 * not registered anywhere, and no store account, signing key or publication exists (Q37 of
 * docs/DECISIONS.md). Once an app is on a store its id cannot change, so the owner rules first.
 *
 * The storage names are what an installed app already has on disk and in its secure store. They
 * are written out and not derived from the id: renaming one opens another file, or loses the key,
 * and the trips of an installed app are gone. `identity.test.ts` pins them.
 *
 * Node reads this file when the app is built, so it imports nothing.
 */
export const identity = {
  applicationId: "org.obop.rutsa",
  version: "0.1.0",
  storage: {
    databaseFile: "rutsa.db",
    databaseKey: "org.obop.rutsa.database-key",
  },
} as const;

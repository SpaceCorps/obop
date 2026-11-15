import { defineIdentity } from "../src/core/host-config.ts";

/**
 * The identity of this build, in one place. `app.config.ts` reads it for the scheme, the Android
 * package, the iOS bundle id and the version; the app reads it through its startup configuration
 * (`config/startup.ts`) for the redirect, the name of the client and the names it keeps on the
 * phone.
 *
 * The application id is a placeholder: no store account, signing key or publication exists (Q37 of
 * docs/DECISIONS.md), and once an app is on a store the id is hard to change.
 *
 * The four storage names are what an installed app already has in its Keychain or Keystore and on
 * disk. They are written out and are not derived from the application id or from each other:
 * changing one loses the stored sign-in, or opens a different database file and leaves the unsent
 * trips behind. `config/identity.test.ts` pins every one.
 *
 * Node reads this file when the app is built, so it imports only `src/core/host-config.ts`, which
 * uses erasable syntax and imports nothing.
 */
export const identity = defineIdentity({
  applicationId: "org.obop.mileage",
  clientName: "obop Mileage",
  version: "0.1.0",
  locales: ["en"],
  defaultServer: "",
  serverLocked: false,
  storage: {
    databaseFile: "mileage.db",
    databaseKey: "obop.mileage.database-key",
    server: "obop.mileage.server",
    tokens: "obop.mileage.tokens",
  },
});

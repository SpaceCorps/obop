/**
 * Constants of the Mileage module. The identity of the app (the application id, the redirect of the
 * sign-in, the name of the client) and the names it keeps on the phone are in `config/identity.ts`
 * and reach the host code through the startup configuration (`native/host.ts`).
 */

/** The groups the app asks for. A phone is offered only what its client declared (DEPLOY.md "Mobile apps"). */
export const SCOPE: readonly string[] = ["mileage.driver"];

/** The name of the background location task. The task is defined once, at the top of `task.ts`. */
export const LOCATION_TASK = "obop-mileage-location";

/** The choice of the language of the app (`system`, `en`, `sv`): a setting, not a secret, kept where it can be read before the database opens. */
export const LANGUAGE_KEY_NAME = "obop.mileage.language";

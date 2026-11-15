/**
 * Constants of the app. `APPLICATION_ID` is a placeholder (Q37 of docs/DECISIONS.md): no store
 * account, signing key or publication exists. The redirect of the sign-in is a private-use scheme
 * in reverse domain form with a path only (RFC 8252 section 7.1), as the server requires.
 */
export const APPLICATION_ID = "org.obop.mileage";
export const REDIRECT_URI = `${APPLICATION_ID}:/oauth`;
export const CLIENT_NAME = "obop Mileage";

/** The groups the app asks for. A phone is offered only what its client declared (DEPLOY.md "Mobile apps"). */
export const SCOPE: readonly string[] = ["mileage.driver"];

/** The name of the background location task. The task is defined once, at the top of `task.ts`. */
export const LOCATION_TASK = "obop-mileage-location";

export const DATABASE_NAME = "mileage.db";
export const DATABASE_KEY_NAME = "obop.mileage.database-key";
export const SERVER_KEY_NAME = "obop.mileage.server";
export const TOKENS_KEY_NAME = "obop.mileage.tokens";

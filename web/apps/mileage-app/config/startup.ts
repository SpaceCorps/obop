import { MILEAGE_SCHEMA } from "../src/core/sql-store.ts";
import { defineStartup } from "../src/core/startup-config.ts";
import { SCOPE } from "../src/native/config.ts";
import { identity } from "./identity.ts";

/**
 * The startup configuration of the Mileage app. Only the entry of the app (`index.ts`) and the
 * tests import this file; the host code is given the object (`startHost`).
 *
 * `legacyGroups` is a list written out on purpose. It is what every build before the groups were
 * stored with the server address registered with, and an installed app that has no stored list is
 * taken to have registered with it. If it followed `SCOPE`, a build that adds a group would look
 * unchanged to an installed app and the sign-in would be refused for the new group.
 */
export const startup = defineStartup({
  identity,
  groups: SCOPE,
  legacyGroups: ["mileage.driver"],
  modules: [MILEAGE_SCHEMA],
});

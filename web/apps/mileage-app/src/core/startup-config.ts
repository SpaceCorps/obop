import { groupsProblem, type HostIdentity } from "./host-config.ts";

/**
 * Everything an app tells the host code when it starts, as one object that the app builds once
 * (`defineStartup`) and hands over (`startHost` in `native/host.ts`). The host code never imports
 * an app's instance of it; it is given the object.
 */
export interface StartupConfig {
  readonly identity: HostIdentity;
  /**
   * The groups the app asks for at the sign-in: the union of what its modules need. The instance
   * offers a phone only the groups that the client declared when it registered.
   */
  readonly groups: readonly string[];
  /**
   * The groups that every build before the list was stored with the server address registered
   * with. A stored config without a list is taken to have registered with these. They are written
   * out and never taken from `groups`: when `groups` grows, an installed app must see that its
   * client was registered with fewer.
   */
  readonly legacyGroups?: readonly string[];
}

export interface StartupInput {
  identity: HostIdentity;
  groups: readonly string[];
  legacyGroups?: readonly string[];
}

/** Builds the startup configuration and checks it; a bad value throws here, when the app starts. */
export function defineStartup(input: StartupInput): StartupConfig {
  const fail = (problem: string): never => {
    throw new Error(`startup: ${problem}`);
  };
  const groups = groupsProblem(input.groups);
  if (groups !== null) fail(`groups: ${groups}`);
  if (input.legacyGroups !== undefined) {
    const legacy = groupsProblem(input.legacyGroups);
    if (legacy !== null) fail(`legacyGroups: ${legacy}`);
  }
  return Object.freeze({
    identity: input.identity,
    groups: Object.freeze([...input.groups]),
    ...(input.legacyGroups === undefined
      ? {}
      : { legacyGroups: Object.freeze([...input.legacyGroups]) }),
  });
}

import type { StartupConfig } from "../core/startup-config.ts";

/**
 * The startup configuration of the app, given to the host code once. The entry of the app
 * (`index.ts`) calls `startHost` at its top level; everything that needs the configuration asks for
 * it when it runs (`hostConfig`) and never when it is imported. That is what lets the background
 * task, which the system starts without any screen and which has no runtime object, find it: the
 * entry has run to its end before the system calls the task.
 *
 * The code of the host does not import the configuration of any app. A thin app that carries the
 * host starts it with its own.
 */
let current: StartupConfig | null = null;

/** Gives the host its configuration. Starting again with the same object is harmless. */
export function startHost(config: StartupConfig): void {
  if (current !== null && current !== config) {
    throw new Error("The host was started again with another configuration");
  }
  current = config;
}

/** The configuration the app started the host with. Throws when it has not, and says so. */
export function hostConfig(): StartupConfig {
  if (current === null) {
    throw new Error(
      "The host has not been started: the entry of the app has to call startHost before the first use of the configuration",
    );
  }
  return current;
}

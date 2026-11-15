// What the page knows about the phone host app around it, and the one way to ask it something.
//
// The back office can be shown inside the phone host app (`web/apps/host-app`), which draws it in a
// WebView and speaks the bridge of `@obop/host-bridge` with it. This file holds what the page has
// learned (whether it is embedded, and the host's answer to `host.info`) as a small store that React
// reads through `useHost`, and the function the rest of the page uses to ask the host. It touches
// no DOM and imports no React, so `passkeys.ts` can read it without pulling the embed code in.
//
// In a browser nothing ever changes here: the state stays "outside", every feature is allowed and
// no command is answered.
import { BridgeError } from "@obop/host-bridge";
import type { CommandMap, CommandName, HostInfo } from "@obop/host-bridge";

/** What the sign-in page may offer only where the host says so. */
export type HostFeature = "passkeys" | "microsoft";

export type HostState = {
  /** The user agent names the host and the transport exists. */
  readonly embedded: boolean;
  /** The host's answer to `host.info`: `null` until it came, and for good when it did not. */
  readonly info: HostInfo | null;
};

/** One request to the host; what `PageBridge.request` is. */
export type Ask = <C extends CommandName>(
  command: C,
  params: CommandMap[C]["params"],
) => Promise<CommandMap[C]["result"]>;

const OUTSIDE: HostState = Object.freeze({ embedded: false, info: null });

let state: HostState = OUTSIDE;
let ask: Ask | null = null;
const listeners = new Set<() => void>();

/** The state now. The same object until something changes, so a store hook can compare it. */
export function hostState(): HostState {
  return state;
}

/** Calls `listener` after every change; the return value stops that. */
export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** For the embed code only: the new state, and whom to tell. */
export function setHostState(next: HostState): void {
  state = Object.freeze({ embedded: next.embedded, info: next.info });
  for (const listener of listeners) listener();
}

/** For the embed code only: how a request reaches the host, or `null` for no way. */
export function setHostAsk(next: Ask | null): void {
  ask = next;
}

/** For the embed code only (and its tests): back to a page outside a host. */
export function leaveHost(): void {
  ask = null;
  if (state !== OUTSIDE) {
    state = OUTSIDE;
    for (const listener of listeners) listener();
  }
}

/**
 * Whether `feature` may be offered in `s`: always outside a host; inside one not until `host.info`
 * has answered, and then what the answer says. A host that never answers allows nothing.
 */
export function allowsIn(s: HostState, feature: HostFeature): boolean {
  if (!s.embedded) return true;
  return s.info !== null && s.info.signIn[feature] === true;
}

export function hostAllows(feature: HostFeature): boolean {
  return allowsIn(state, feature);
}

/**
 * Whether the host has said that it answers `command`. A page calls nothing that is not on the
 * host's list, so this is false outside a host and until `host.info` has answered. The name is
 * text, not a `CommandName`: a later host may list commands this version does not know.
 */
export function answersIn(s: HostState, command: string): boolean {
  return s.embedded && s.info !== null && (s.info.commands as readonly string[]).includes(command);
}

export function hostAnswers(command: string): boolean {
  return answersIn(state, command);
}

/**
 * Whether a file the page offers as a download reaches the person: always in a browser; inside a
 * host only when the host has said that it answers `file.save`. While this is false the embed code
 * stops every download and says so (`downloads.ts`), and the page reports no saved file.
 */
export function savesIn(s: HostState): boolean {
  return !s.embedded || answersIn(s, "file.save");
}

export function hostSavesFiles(): boolean {
  return savesIn(state);
}

/**
 * Asks the host to run `command`. The promise ends in the command's result or in a `BridgeError`;
 * a command the host did not list (and every command outside a host) is not sent at all.
 */
export function askHost<C extends CommandName>(
  command: C,
  params: CommandMap[C]["params"],
): Promise<CommandMap[C]["result"]> {
  if (ask === null || !hostAnswers(command))
    return Promise.reject(new BridgeError("unknown_command", "The app does not answer that."));
  return ask(command, params);
}

// The host's side of the bridge: the three commands of version 1 and the pump that carries a
// message of the page to them and the answer back.
//
// The handler list is the allow-list: a command that is not a key of it cannot reach native code.
// A module adds handlers named `<module id>.<verb>` here later, and `host.info.commands` then lists
// them, which is how the page knows. No version 1 command has power, and no answer holds a secret.
import {
  deliveryScript,
  type CommandName,
  type Handlers,
  type HostBridge,
  type HostInfo,
  type Platform,
} from "@obop/host-bridge";
import type { Build } from "./build.ts";
import type { PageTheme } from "./theme.ts";

export interface CommandDeps {
  readonly platform: Platform;
  /** The application id of the build. */
  readonly appId: string;
  /** The version of the app. */
  readonly version: string;
  readonly build: Build;
  /** `host.theme`: the page reports its ground colour and whether it is a dark one. */
  readonly onTheme: (theme: PageTheme) => void;
  /** `host.settings`: the page asks for the native "This app" screen. */
  readonly onSettings: () => void;
}

/**
 * What `host.info` answers. `signIn` says what the sign-in page may offer inside this host:
 * neither a passkey nor the Microsoft round trip works in the WebView, so both are `false` and the
 * page draws neither button. `app` carries both flags of a test build.
 */
export function hostInfo(deps: CommandDeps, commands: readonly CommandName[]): HostInfo {
  return {
    bridge: 1,
    platform: deps.platform,
    app: {
      id: deps.appId,
      version: deps.version,
      cleartext: deps.build.cleartext,
      inspectable: deps.build.webviewDebug,
    },
    commands,
    signIn: { passkeys: false, microsoft: false },
  };
}

/**
 * The handlers of version 1. The list that `host.info` gives is read from the handlers themselves,
 * so it cannot name a command the host does not answer.
 */
export function hostHandlers(deps: CommandDeps): Handlers {
  const handlers: Handlers = {
    "host.info": () => hostInfo(deps, Object.keys(handlers) as CommandName[]),
    "host.theme": (params) => {
      deps.onTheme({ scheme: params.scheme, background: params.background });
      return {};
    },
    "host.settings": () => {
      deps.onSettings();
      return {};
    },
  };
  return handlers;
}

export interface PumpDeps {
  readonly bridge: HostBridge;
  /**
   * Whether the host answers at this instant: the document on top is the instance's, in the
   * WebView that is mounted now. It is asked twice per message, because an answer is injected into
   * whatever document is current when it is delivered.
   */
  readonly answering: () => boolean;
  /** Evaluates a script in the current document of the WebView. */
  readonly inject: (script: string) => void;
}

/**
 * Carries one message of the page to the bridge and the answer back. While a foreign document is
 * or was on top, the message is dropped before the bridge sees it and nothing is injected. The
 * sender's origin is the bridge's own check.
 */
export function createMessagePump(
  deps: PumpDeps,
): (data: string, senderUrl: string | undefined) => Promise<void> {
  return async (data, senderUrl) => {
    if (!deps.answering()) return;
    const answer = await deps.bridge.handle(data, senderUrl);
    if (answer === null) return;
    if (!deps.answering()) return;
    deps.inject(deliveryScript(answer));
  };
}

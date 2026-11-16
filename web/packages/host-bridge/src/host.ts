// The host's side of the bridge: it takes one message of a page and gives the text of the answer,
// or nothing. The app decides before it calls `handle` whether the document on top is the
// instance's (its `top` state); everything else about a message is decided here.
import { originOf } from "./origin.ts";
import {
  BRIDGE_VERSION,
  MAX_ERROR_MESSAGE_CHARS,
  MAX_MESSAGE_CHARS,
  isCommandName,
  isRequestId,
  parseMessage,
  validParams,
  validResult,
} from "./protocol.ts";
import type { BridgeResponse, CommandMap, CommandName, ErrorCode } from "./protocol.ts";

/**
 * The commands a host answers. This list is the allow-list: a command that is not an own key of it
 * cannot reach native code, and there is no other way in.
 */
export type Handlers = {
  [C in CommandName]?: (
    params: CommandMap[C]["params"],
  ) => Promise<CommandMap[C]["result"]> | CommandMap[C]["result"];
};

/**
 * What a handler throws to say no in its own words: `refused` (understood and not done) or
 * `failed` (tried and could not). The message goes to the page, so it holds nothing secret.
 * Anything else a handler throws is answered with `failed` and a fixed text.
 */
export class Refusal extends Error {
  readonly code: "refused" | "failed";

  constructor(code: "refused" | "failed", message: string) {
    super(message);
    this.name = "Refusal";
    this.code = code;
  }
}

export interface HostBridge {
  /** The JSON text to hand back to the page, or `null` for no answer. Never throws. */
  handle(data: string, senderUrl: string | undefined): Promise<string | null>;
}

/** The fixed texts of the answers. None holds anything of the message or of a thrown error. */
const TEXT = {
  version: "This host speaks version 1 of the bridge.",
  command: "This host does not answer that command.",
  params: "The parameters are not what the command takes.",
  failed: "The host could not do that.",
} as const;

function failure(id: string, code: ErrorCode, message: string): string {
  const response: BridgeResponse = {
    obop: BRIDGE_VERSION,
    kind: "response",
    id,
    ok: false,
    error: { code, message: message.slice(0, MAX_ERROR_MESSAGE_CHARS) },
  };
  return JSON.stringify(response);
}

async function answer(
  id: string,
  command: CommandName,
  handler: (params: unknown) => unknown,
  params: unknown,
): Promise<string> {
  // (5) The parameters, by the hand-written check of the command. An extra key is refused.
  if (!validParams(command, params)) return failure(id, "bad_params", TEXT.params);
  // (6) The handler. Only a Refusal's own words reach the page.
  let result: unknown;
  try {
    result = await handler(params);
  } catch (error) {
    if (error instanceof Refusal) return failure(id, error.code, error.message);
    return failure(id, "failed", TEXT.failed);
  }
  if (!validResult(command, result)) return failure(id, "failed", TEXT.failed);
  let text: string;
  try {
    text = JSON.stringify({ obop: BRIDGE_VERSION, kind: "response", id, ok: true, result });
  } catch {
    return failure(id, "failed", TEXT.failed);
  }
  // The page drops a longer message, so it is told that the command failed instead.
  return text.length <= MAX_MESSAGE_CHARS ? text : failure(id, "failed", TEXT.failed);
}

/**
 * The host's side for one instance. `origin` is the stored origin of the instance as `originOf`
 * writes one (`https://erp.example.com`, `http://localhost:18180`); anything else throws here.
 *
 * `now` is not read by version 1. It is in the signature so that a later rule about how often a
 * page may ask needs no change in the callers.
 */
export function createHostBridge(deps: {
  origin: string;
  handlers: Handlers;
  now?: () => number;
}): HostBridge {
  const { origin, handlers } = deps;
  if (originOf(origin) !== origin) {
    throw new Error(`createHostBridge: ${JSON.stringify(origin)} is not an origin`);
  }

  async function handle(data: string, senderUrl: string | undefined): Promise<string | null> {
    // (1) The sender. A frame of another origin, a document without an origin and a message
    // without a sender get no answer.
    if (originOf(senderUrl) !== origin) return null;
    // (2) Not a request of this bridge at all: no answer.
    const message = parseMessage(data);
    if (message === null) return null;
    const id = message.id;
    if (!isRequestId(id)) return null;
    if (message.kind !== "request") return null;
    // (3) The version of the envelope.
    if (message.obop !== BRIDGE_VERSION) return failure(id, "unsupported_version", TEXT.version);
    // (4) The command: a name of this version that is an own key of the handler list.
    const command = message.command;
    if (!isCommandName(command) || !Object.prototype.hasOwnProperty.call(handlers, command)) {
      return failure(id, "unknown_command", TEXT.command);
    }
    const handler: unknown = handlers[command];
    if (typeof handler !== "function") return failure(id, "unknown_command", TEXT.command);
    return answer(id, command, handler as (params: unknown) => unknown, message.params);
  }

  return {
    async handle(data, senderUrl) {
      try {
        return await handle(data, senderUrl);
      } catch {
        return null;
      }
    },
  };
}

/**
 * The script that delivers one answer: it calls `window.__obopHost.receive` when it exists. The
 * text is passed as a string literal; `<`, `>` and the two Unicode line separators are escaped, so
 * the literal cannot end early wherever the script is evaluated. The script ends in `true;`
 * because an injected script has to evaluate to a value the WebView can hand back.
 */
export function deliveryScript(json: string): string {
  const literal = JSON.stringify(json).replace(
    /[<>\u2028\u2029]/g,
    (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `(function(){var h=window.__obopHost;if(h&&typeof h.receive==="function"){try{h.receive(${literal});}catch(e){}}})();true;`;
}

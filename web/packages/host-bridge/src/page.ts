// The page's side of the bridge. The page builds one of these from its user agent and from the
// function the WebView gives it for posting a message, and hands every text the host delivers to
// `receive`. Nothing here reads a global of the page: both are passed in.
import {
  BRIDGE_VERSION,
  MAX_ERROR_MESSAGE_CHARS,
  MAX_MESSAGE_CHARS,
  REQUEST_TIMEOUT_MS,
  USER_AGENT_TOKEN,
  isErrorCode,
  isRecord,
  isRequestId,
  parseMessage,
  validParams,
  validResult,
} from "./protocol.ts";
import type { BridgeRequest, CommandMap, CommandName, ErrorCode } from "./protocol.ts";

/** What a request ends in when it does not end in a result. */
export class BridgeError extends Error {
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = "BridgeError";
    this.code = code;
  }
}

export interface PageBridge {
  /** The user agent names the host and the transport exists. */
  readonly embedded: boolean;
  request<C extends CommandName>(
    command: C,
    params: CommandMap[C]["params"],
    options?: { timeoutMs?: number },
  ): Promise<CommandMap[C]["result"]>;
  /** What `window.__obopHost.receive` calls with one JSON text. */
  receive(json: string): void;
}

/** `ObopHost/<n>` as a word of its own in the user agent, `n` without a leading zero. */
const HOST_TOKEN = new RegExp(
  `(?:^|[\\s;(])${USER_AGENT_TOKEN}/([1-9][0-9]{0,3})(?![0-9A-Za-z_.])`,
);

/** The major version of the bridge that the user agent names, or `null`: `ObopHost/1` gives 1. */
export function hostBridgeVersion(userAgent: string): number | null {
  if (typeof userAgent !== "string") return null;
  const match = HOST_TOKEN.exec(userAgent);
  return match === null ? null : Number(match[1]);
}

/** The fixed texts of the errors that the page's own side makes. */
const TEXT = {
  outside: "This page is not inside the obop app.",
  version: "The app speaks another version of the bridge.",
  params: "The parameters are not what the command takes.",
  post: "The message could not be handed to the app.",
  timeout: "The app did not answer in time.",
  answer: "The app's answer could not be read.",
} as const;

interface Pending {
  command: CommandName;
  resolve: (result: unknown) => void;
  reject: (error: BridgeError) => void;
  timer: ReturnType<typeof setTimeout>;
}

export function createPageBridge(deps: {
  userAgent: string;
  post: ((json: string) => void) | null;
}): PageBridge {
  const version = hostBridgeVersion(deps.userAgent);
  const post = typeof deps.post === "function" ? deps.post : null;
  const embedded = version !== null && post !== null;
  const pending = new Map<string, Pending>();
  let counter = 0;

  /** Takes a request off the list and stops its clock. `undefined` when it is not waiting. */
  function take(id: string): Pending | undefined {
    const entry = pending.get(id);
    if (entry === undefined) return undefined;
    pending.delete(id);
    clearTimeout(entry.timer);
    return entry;
  }

  function request<C extends CommandName>(
    command: C,
    params: CommandMap[C]["params"],
    options?: { timeoutMs?: number },
  ): Promise<CommandMap[C]["result"]> {
    if (!embedded || post === null) {
      return Promise.reject(new BridgeError("failed", TEXT.outside));
    }
    if (version !== BRIDGE_VERSION) {
      return Promise.reject(new BridgeError("unsupported_version", TEXT.version));
    }
    if (!validParams(command, params)) {
      return Promise.reject(new BridgeError("bad_params", TEXT.params));
    }
    counter += 1;
    const id = `p${counter}`;
    const message: BridgeRequest<C> = {
      obop: BRIDGE_VERSION,
      kind: "request",
      id,
      command,
      params,
    };
    const json = JSON.stringify(message);
    if (json.length > MAX_MESSAGE_CHARS) {
      return Promise.reject(new BridgeError("bad_params", TEXT.params));
    }
    const asked = options?.timeoutMs;
    const timeoutMs =
      typeof asked === "number" && Number.isFinite(asked) && asked > 0 ? asked : REQUEST_TIMEOUT_MS;

    return new Promise<CommandMap[C]["result"]>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (take(id) !== undefined) reject(new BridgeError("failed", TEXT.timeout));
      }, timeoutMs);
      pending.set(id, {
        command,
        resolve: resolve as (result: unknown) => void,
        reject,
        timer,
      });
      try {
        post(json);
      } catch {
        if (take(id) !== undefined) reject(new BridgeError("failed", TEXT.post));
      }
    });
  }

  function receive(json: string): void {
    try {
      const message = parseMessage(json);
      if (message === null || message.obop !== BRIDGE_VERSION) return;
      // An event: the first slice has none, and a page ignores an event it does not know.
      if (message.kind !== "response") return;
      const id = message.id;
      if (!isRequestId(id)) return;
      // An answer nobody waits for (a second one, or one that came after the clock ran out).
      const entry = take(id);
      if (entry === undefined) return;
      if (message.ok === true) {
        if (validResult(entry.command, message.result)) entry.resolve(message.result);
        else entry.reject(new BridgeError("failed", TEXT.answer));
        return;
      }
      const error = message.error;
      if (
        message.ok === false &&
        isRecord(error) &&
        isErrorCode(error.code) &&
        typeof error.message === "string"
      ) {
        entry.reject(new BridgeError(error.code, error.message.slice(0, MAX_ERROR_MESSAGE_CHARS)));
        return;
      }
      entry.reject(new BridgeError("failed", TEXT.answer));
    } catch {
      // `receive` is called from a script the host injects; nothing may come back out of it.
    }
  }

  return { embedded, request, receive };
}

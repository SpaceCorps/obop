// The bridge between the obop phone host app and the back office page it shows, version 1: the
// types, the constants and the hand-written validators that both sides use. This file imports
// nothing and names no global of one platform (src/neutral.test.ts).

export const BRIDGE_VERSION = 1;
/** In the WebView's user agent: `ObopHost/1 (android; 0.1.0)`. */
export const USER_AGENT_TOKEN = "ObopHost";
/** A message longer than this is dropped by either side. */
export const MAX_MESSAGE_CHARS = 65_536;
/** How long the page waits for an answer. */
export const REQUEST_TIMEOUT_MS = 5_000;
/** The object the page defines once on its window; the host hands an answer to its `receive`. */
export const PAGE_RECEIVER = "__obopHost";

export type Platform = "android" | "ios";
export type Scheme = "light" | "dark";
type Empty = Record<string, never>;

export interface HostInfo {
  bridge: 1;
  platform: Platform;
  /** `cleartext`: the build may speak plain http. `inspectable`: the WebView is open to DevTools. */
  app: { id: string; version: string; cleartext: boolean; inspectable: boolean };
  /** The commands this host answers. A page calls nothing that is not listed. */
  commands: readonly CommandName[];
  /** What the sign-in page may offer inside this host. */
  signIn: { passkeys: boolean; microsoft: boolean };
}

export interface CommandMap {
  /** The first call of a page. */
  "host.info": { params: Empty; result: HostInfo };
  /** The page's ground colour and whether it is a dark one. `background` is `#rrggbb`. */
  "host.theme": { params: { scheme: Scheme; background: string }; result: Empty };
  /** Opens the native "This app" screen. */
  "host.settings": { params: Empty; result: Empty };
}
export type CommandName = keyof CommandMap;

export type ErrorCode =
  | "unsupported_version" // `obop` is not 1
  | "unknown_command" // not on the host's list
  | "bad_params" // a missing, extra or wrong-typed field
  | "refused" // understood and not done
  | "failed"; // the host tried and could not, or no answer came in time

/** Page to host. `id`: 1 to 64 of `A-Z a-z 0-9 _ -`, unique in a page load. */
export interface BridgeRequest<C extends CommandName = CommandName> {
  obop: 1;
  kind: "request";
  id: string;
  command: C;
  params: CommandMap[C]["params"];
}

/** Host to page: the answer to one request. */
export type BridgeResponse<C extends CommandName = CommandName> =
  | { obop: 1; kind: "response"; id: string; ok: true; result: CommandMap[C]["result"] }
  | {
      obop: 1;
      kind: "response";
      id: string;
      ok: false;
      error: { code: ErrorCode; message: string };
    };

/** Host to page, unasked. The first slice has none; a page ignores an event it does not know. */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface EventMap {}
export interface BridgeEvent<E extends keyof EventMap = keyof EventMap> {
  obop: 1;
  kind: "event";
  event: E;
  data: EventMap[E];
}

// What follows is checked at run time. A message is text from the other side of a WebView, so
// nothing about it is taken from its TypeScript type.

/** The commands of version 1, in the order of `CommandMap`. */
export const COMMAND_NAMES: readonly CommandName[] = ["host.info", "host.theme", "host.settings"];

export const ERROR_CODES: readonly ErrorCode[] = [
  "unsupported_version",
  "unknown_command",
  "bad_params",
  "refused",
  "failed",
];

/** The longest text of an error that either side passes on. */
export const MAX_ERROR_MESSAGE_CHARS = 500;

const REQUEST_ID = /^[A-Za-z0-9_-]{1,64}$/;
const HEX_COLOUR = /^#[0-9a-fA-F]{6}$/;
// `<prefix>.<verb>`: a later host may list commands this version does not know (`file.save`,
// `mileage.start`, `link.openExternal`), so a listed name is checked by its form only.
const COMMAND_FORM = /^[a-z][a-z0-9_]{0,31}\.[A-Za-z][A-Za-z0-9_]{0,31}$/;
/** The most commands a host may list in `host.info`. */
const MAX_LISTED_COMMANDS = 128;

export function isRequestId(value: unknown): value is string {
  return typeof value === "string" && REQUEST_ID.test(value);
}

export function isCommandName(value: unknown): value is CommandName {
  return typeof value === "string" && (COMMAND_NAMES as readonly string[]).includes(value);
}

export function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === "string" && (ERROR_CODES as readonly string[]).includes(value);
}

/** `#rrggbb`, in either case. */
export function isHexColour(value: unknown): value is string {
  return typeof value === "string" && HEX_COLOUR.test(value);
}

/** An object that is not `null` and not an array. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** The own keys of `value` are exactly `keys`: none missing and none extra. */
function hasExactly(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const own = Object.keys(value);
  return own.length === keys.length && keys.every((key) => own.includes(key));
}

function isEmptyParams(value: unknown): value is Empty {
  return isRecord(value) && hasExactly(value, []);
}

function isThemeParams(value: unknown): value is CommandMap["host.theme"]["params"] {
  return (
    isRecord(value) &&
    hasExactly(value, ["scheme", "background"]) &&
    (value.scheme === "light" || value.scheme === "dark") &&
    isHexColour(value.background)
  );
}

/**
 * A result object. A host of a later release may add an optional field without a new major
 * version, so a result may hold keys this version does not know; the fields it does know must be
 * there and of the right type.
 */
function isEmptyResult(value: unknown): value is Empty {
  return isRecord(value);
}

function isShortText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= max;
}

function isHostInfo(value: unknown): value is HostInfo {
  if (!isRecord(value)) return false;
  if (value.bridge !== BRIDGE_VERSION) return false;
  if (value.platform !== "android" && value.platform !== "ios") return false;
  const app = value.app;
  if (!isRecord(app)) return false;
  if (!isShortText(app.id, 256) || !isShortText(app.version, 64)) return false;
  if (typeof app.cleartext !== "boolean" || typeof app.inspectable !== "boolean") return false;
  const commands = value.commands;
  if (!Array.isArray(commands) || commands.length > MAX_LISTED_COMMANDS) return false;
  if (!commands.every((name) => typeof name === "string" && COMMAND_FORM.test(name))) return false;
  const signIn = value.signIn;
  if (!isRecord(signIn)) return false;
  return typeof signIn.passkeys === "boolean" && typeof signIn.microsoft === "boolean";
}

type ParamChecks = {
  readonly [C in CommandName]: (value: unknown) => value is CommandMap[C]["params"];
};
type ResultChecks = {
  readonly [C in CommandName]: (value: unknown) => value is CommandMap[C]["result"];
};

const PARAM_CHECKS: ParamChecks = {
  "host.info": isEmptyParams,
  "host.theme": isThemeParams,
  "host.settings": isEmptyParams,
};

const RESULT_CHECKS: ResultChecks = {
  "host.info": isHostInfo,
  "host.theme": isEmptyResult,
  "host.settings": isEmptyResult,
};

/** The parameters of `command` are exactly what the command takes: a missing, extra or wrong-typed field is refused. */
export function validParams<C extends CommandName>(
  command: C,
  value: unknown,
): value is CommandMap[C]["params"] {
  return PARAM_CHECKS[command](value);
}

/** The result of `command` holds every field of this version with the right type. */
export function validResult<C extends CommandName>(
  command: C,
  value: unknown,
): value is CommandMap[C]["result"] {
  return RESULT_CHECKS[command](value);
}

/**
 * One message as an object, or `null`: text of at most `MAX_MESSAGE_CHARS` that is JSON and an
 * object. Either side drops anything else without an answer.
 */
export function parseMessage(text: unknown): Record<string, unknown> | null {
  if (typeof text !== "string" || text.length > MAX_MESSAGE_CHARS) return null;
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  return isRecord(value) ? value : null;
}

// @obop/host-bridge: the bridge between the obop phone host app and the back office page it shows,
// version 1. The app (`createHostBridge`), the back office (`createPageBridge`) and the stand-in
// host of the end-to-end tests import this same code.
export {
  BRIDGE_VERSION,
  COMMAND_NAMES,
  ERROR_CODES,
  MAX_ERROR_MESSAGE_CHARS,
  MAX_MESSAGE_CHARS,
  PAGE_RECEIVER,
  REQUEST_TIMEOUT_MS,
  USER_AGENT_TOKEN,
  isCommandName,
  isErrorCode,
  isHexColour,
  isRequestId,
  parseMessage,
  validParams,
  validResult,
} from "./protocol.ts";
export type {
  BridgeEvent,
  BridgeRequest,
  BridgeResponse,
  CommandMap,
  CommandName,
  ErrorCode,
  EventMap,
  HostInfo,
  Platform,
  Scheme,
} from "./protocol.ts";
export { originOf } from "./origin.ts";
export { BridgeError, createPageBridge, hostBridgeVersion } from "./page.ts";
export type { PageBridge } from "./page.ts";
export { Refusal, createHostBridge, deliveryScript } from "./host.ts";
export type { Handlers, HostBridge } from "./host.ts";

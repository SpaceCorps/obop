// What the app adds to the WebView's user agent. It is how the page knows from its first request
// that it is inside the host; the page also needs the transport before it calls itself embedded.
import { BRIDGE_VERSION, USER_AGENT_TOKEN, type Platform } from "@obop/host-bridge";

/** `ObopHost/1 (android; 0.1.0)`: the major version of the bridge, the system, the app's version. */
export function userAgentSuffix(platform: Platform, version: string): string {
  return `${USER_AGENT_TOKEN}/${BRIDGE_VERSION} (${platform}; ${version})`;
}

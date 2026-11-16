import type { Handlers, Platform as HostPlatform } from "@obop/host-bridge";
import Constants from "expo-constants";
import { Platform } from "react-native";
import { identity } from "../../config/identity.ts";
import { hostHandlers } from "../core/commands.ts";
import type { PageTheme } from "../core/theme.ts";
import { build } from "./build.ts";

/** The system as the bridge names it. The app is built for Android and iOS only. */
export const hostPlatform: HostPlatform = Platform.OS === "ios" ? "ios" : "android";

/** The id the build is installed under: the application id, or on iOS the bundle id of the build. */
function appId(): string {
  const bundleId: unknown = Constants.expoConfig?.ios?.bundleIdentifier;
  return hostPlatform === "ios" && typeof bundleId === "string" && bundleId !== ""
    ? bundleId
    : identity.applicationId;
}

/**
 * The three handlers of version 1 with what only the native side knows: the system, the id and
 * the flags of the build. The handlers themselves are `src/core/commands.ts`.
 */
export function nativeHandlers(on: {
  theme: (theme: PageTheme) => void;
  settings: () => void;
}): Handlers {
  return hostHandlers({
    platform: hostPlatform,
    appId: appId(),
    version: identity.version,
    build,
    onTheme: on.theme,
    onSettings: on.settings,
  });
}

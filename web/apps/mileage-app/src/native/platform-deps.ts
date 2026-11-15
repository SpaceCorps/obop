import type { DeviceInfo, OAuthDeps } from "@obop/client";
import * as Crypto from "expo-crypto";
import { Platform } from "react-native";
import { deviceName } from "../core/device.ts";
import { hostConfig } from "./host.ts";

/** SHA-256 and a secure random source for PKCE, from expo-crypto (native). */
export const oauthDeps: OAuthDeps = {
  sha256: async (text) =>
    new Uint8Array(
      await Crypto.digest(Crypto.CryptoDigestAlgorithm.SHA256, new TextEncoder().encode(text)),
    ),
  randomBytes: (count) => Crypto.getRandomBytesAsync(count),
};

/** The version of the app: the one its identity gives, which is also the version of the build (`app.config.ts`). */
export const appVersion = (): string => hostConfig().identity.version;

/** What the instance learns about this phone at sign-in: a name for the Devices list. */
export function deviceInfo(): DeviceInfo {
  return { name: deviceName(Platform), platform: Platform.OS, appVersion: appVersion() };
}

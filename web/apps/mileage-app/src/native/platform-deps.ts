import type { DeviceInfo, OAuthDeps } from "@obop/client";
import Constants from "expo-constants";
import * as Crypto from "expo-crypto";
import { Platform } from "react-native";
import { deviceName } from "../core/device.ts";

/** SHA-256 and a secure random source for PKCE, from expo-crypto (native). */
export const oauthDeps: OAuthDeps = {
  sha256: async (text) =>
    new Uint8Array(
      await Crypto.digest(Crypto.CryptoDigestAlgorithm.SHA256, new TextEncoder().encode(text)),
    ),
  randomBytes: (count) => Crypto.getRandomBytesAsync(count),
};

export const appVersion = (): string => Constants.expoConfig?.version ?? "0.0.0";

/** What the instance learns about this phone at sign-in: a name for the Devices list. */
export function deviceInfo(): DeviceInfo {
  return { name: deviceName(Platform), platform: Platform.OS, appVersion: appVersion() };
}

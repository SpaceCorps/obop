import { type TokenSet } from "@obop/client";
import type { ServerConfig, SessionStorage } from "../core/session.ts";
import { SERVER_KEY_NAME, TOKENS_KEY_NAME } from "./config.ts";
import { keychain } from "./keychain.ts";

async function read<T>(key: string, valid: (v: unknown) => v is T): Promise<T | null> {
  const text = await keychain.get(key);
  if (!text) return null;
  try {
    const value: unknown = JSON.parse(text);
    return valid(value) ? value : null;
  } catch {
    return null;
  }
}

async function write(key: string, value: unknown): Promise<void> {
  if (value === null) await keychain.delete(key);
  else await keychain.set(key, JSON.stringify(value));
}

const isConfig = (v: unknown): v is ServerConfig =>
  typeof v === "object" &&
  v !== null &&
  typeof (v as ServerConfig).baseUrl === "string" &&
  typeof (v as ServerConfig).clientId === "string";

const isTokens = (v: unknown): v is TokenSet =>
  typeof v === "object" &&
  v !== null &&
  typeof (v as TokenSet).accessToken === "string" &&
  typeof (v as TokenSet).refreshToken === "string" &&
  typeof (v as TokenSet).expiresAt === "number";

/**
 * The session in the Android Keystore or the iOS Keychain (expo-secure-store, through
 * `keychain.ts`: after first unlock, this device only). The tokens are never logged.
 */
export const secureStorage: SessionStorage = {
  loadConfig: () => read(SERVER_KEY_NAME, isConfig),
  saveConfig: (config) => write(SERVER_KEY_NAME, config),
  loadTokens: () => read(TOKENS_KEY_NAME, isTokens),
  saveTokens: (tokens) => write(TOKENS_KEY_NAME, tokens),
};

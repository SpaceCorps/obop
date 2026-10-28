import { OAuthError, refreshTokens, type OAuthDeps, type TokenSet } from "@obop/client";

/** The instance the phone signs in to, and the client the instance gave it. Nothing secret. */
export interface ServerConfig {
  baseUrl: string;
  clientId: string;
  /** The person this phone is signed in as (the owner of its device record), once known. */
  user?: string;
  /** When a sign-in last finished with this client (ms). The instance forgets a client that no sign-in used. */
  usedAt?: number;
}

/** The instance forgets a registered client that nothing has used for a day, and a used one after about 180 idle days. */
export const CLIENT_FRESH_MS = 30 * 24 * 3600 * 1000;

/**
 * Whether the app has to register with the instance before it opens the sign-in page: it has no
 * client for this address, the client was never used by a finished sign-in (the instance forgets
 * those after 24 hours), or the last use is old enough that the instance may have forgotten it.
 */
export function needsRegistration(
  config: ServerConfig | null,
  baseUrl: string,
  nowMs: number,
): boolean {
  if (config === null || config.baseUrl !== baseUrl) return true;
  return config.usedAt === undefined || nowMs - config.usedAt > CLIENT_FRESH_MS;
}

/** Where the session is kept. On the phone: the Android Keystore through expo-secure-store. */
export interface SessionStorage {
  loadConfig(): Promise<ServerConfig | null>;
  saveConfig(config: ServerConfig | null): Promise<void>;
  loadTokens(): Promise<TokenSet | null>;
  saveTokens(tokens: TokenSet | null): Promise<void>;
}

export type SessionStatus =
  /** Nobody is signed in (never, or the driver signed out). */
  | "signed_out"
  | "signed_in"
  /** The instance ended the sign-in (revoked, or the refresh was refused): sign in again. */
  | "expired";

export interface Session {
  status(): SessionStatus;
  config(): ServerConfig | null;
  /** The `device` record of this sign-in, or `null`. */
  deviceId(): string | null;
  /**
   * The access token to send, refreshed first when it is about to end. `null` when nobody is
   * signed in. A refresh that fails because the connection is down throws (the call is then not
   * sent and counts as offline); one the instance refuses ends the sign-in.
   */
  accessToken(): Promise<string | null>;
  /** The instance refused the access token (401): drop it so the next call refreshes. */
  rejected(): Promise<void>;
  /** Reads the stored session. Call once at start. */
  restore(): Promise<void>;
  setConfig(config: ServerConfig): Promise<void>;
  completeSignIn(tokens: TokenSet): Promise<void>;
  /** Forgets the tokens (the instance was told, or could not be). The server config stays. */
  signOut(): Promise<void>;
  refreshToken(): string | null;
  subscribe(listener: () => void): () => void;
}

export interface SessionOptions {
  storage: SessionStorage;
  oauth: OAuthDeps;
  now?: () => number;
  /** An access token is refreshed when it has less than this left. */
  skewMs?: number;
}

/** The answers of the token endpoint that mean the sign-in is over (RFC 6749 section 5.2). */
const ENDED: readonly string[] = [
  "invalid_grant",
  "invalid_client",
  "unauthorized_client",
  "invalid_scope",
];

export function createSession({
  storage,
  oauth,
  now = Date.now,
  skewMs = 60_000,
}: SessionOptions): Session {
  let status: SessionStatus = "signed_out";
  let config: ServerConfig | null = null;
  let tokens: TokenSet | null = null;
  let refreshing: Promise<string | null> | null = null;
  const listeners = new Set<() => void>();

  const change = (next: SessionStatus) => {
    const differs = next !== status;
    status = next;
    if (differs) for (const l of listeners) l();
  };

  async function refresh(): Promise<string | null> {
    if (!config || !tokens) return null;
    const { baseUrl, clientId } = config;
    try {
      const next = await refreshTokens(oauth, {
        baseUrl,
        clientId,
        refreshToken: tokens.refreshToken,
        now: now(),
      });
      // The refresh token is spent: the new pair is kept in memory first, so that a failed write
      // never leaves the spent one in use (the instance ends the whole sign-in when a refresh
      // token is presented twice), and then written, twice if need be.
      tokens = next;
      change("signed_in");
      try {
        await storage.saveTokens(next);
      } catch {
        try {
          await storage.saveTokens(next);
        } catch {
          // The Keystore still holds the spent pair: the next start of the app will find the
          // sign-in ended and ask for a new one. Nothing else can be done here.
        }
      }
      return next.accessToken;
    } catch (error) {
      if (error instanceof OAuthError && ENDED.includes(error.code)) {
        // The instance ended this sign-in. Only the tokens go; the trips on the phone stay.
        tokens = null;
        await storage.saveTokens(null);
        change("expired");
        return null;
      }
      throw error;
    }
  }

  return {
    status: () => status,
    config: () => config,
    deviceId: () => tokens?.deviceId ?? null,
    refreshToken: () => tokens?.refreshToken ?? null,

    async accessToken() {
      if (!tokens) return null;
      if (tokens.expiresAt - now() > skewMs) return tokens.accessToken;
      // Calls that meet an old token at the same moment share one refresh: a second use of the
      // same refresh token would end the whole sign-in.
      refreshing ??= refresh().finally(() => {
        refreshing = null;
      });
      return refreshing;
    },

    async rejected() {
      if (tokens) tokens = { ...tokens, expiresAt: 0 };
    },

    async restore() {
      config = await storage.loadConfig();
      tokens = await storage.loadTokens();
      status = tokens && config ? "signed_in" : "signed_out";
      for (const l of listeners) l();
    },

    async setConfig(next) {
      await storage.saveConfig(next);
      config = next;
    },

    async completeSignIn(next) {
      await storage.saveTokens(next);
      tokens = next;
      change("signed_in");
    },

    async signOut() {
      tokens = null;
      await storage.saveTokens(null);
      change("signed_out");
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

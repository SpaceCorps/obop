import {
  OAuthError,
  refreshTokens,
  revokeToken,
  type OAuthDeps,
  type TokenSet,
} from "@obop/client";

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

/**
 * What the authorization page answers for a client that the instance does not know (the instance
 * forgot it for a reason other than idleness: its database was replaced, a restore, a new
 * installation at the same address): 400 and an error page, without a redirect. A client that it
 * knows gets the sign-in page. The app asks before it opens the browser, and registers again once
 * when the answer is this one, so that the driver never lands on an error page that the app cannot
 * read (the browser reports only that it was closed).
 */
export const clientUnknown = (status: number | null): boolean => status === 400;

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

/**
 * Why a sign-in ended without the driver asking: the instance refused the refresh (`refused`), or
 * the refresh was given up after a timeout and the phone cannot tell whether the instance rotated
 * the token (`unsure`).
 */
export type EndedBy = "refused" | "unsure";

export interface Session {
  status(): SessionStatus;
  /** Why the sign-in ended, while the status is `expired`; `null` otherwise. */
  endedBy(): EndedBy | null;
  /**
   * A number that changes whenever the sign-in changes: completed, signed out, ended, restored.
   * Work that started under one value must not go on under another (a pass of the outbox that was
   * running when the driver signed out must not send the next trip with the next person's token).
   */
  generation(): number;
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
  /** A refresh that has no answer after this long is given up (the outcome is then unknown). */
  refreshTimeoutMs?: number;
  /**
   * The instance could not be told, in the background, that a sign-in the phone ended on its own
   * (`unsure`) is over: the instance still lists the phone. Not called when a newer sign-in has
   * finished (`completeSignIn` was called) by the time the revoke fails; one that is still in the
   * browser does not stop it.
   */
  onEndUntold?: () => void;
}

/** The longest a refresh waits: longer than a slow cellular answer, and the outcome is unknown after it. */
export const REFRESH_TIMEOUT_MS = 60_000;

/** How long the best-effort end of a sign-in at the instance may take. */
const END_REVOKE_MS = 5000;

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
  refreshTimeoutMs = REFRESH_TIMEOUT_MS,
  onEndUntold,
}: SessionOptions): Session {
  let status: SessionStatus = "signed_out";
  let ended: EndedBy | null = null;
  let generation = 0;
  let config: ServerConfig | null = null;
  let tokens: TokenSet | null = null;
  let refreshing: Promise<string | null> | null = null;
  const listeners = new Set<() => void>();

  // Writes to the Keystore go one after the other, so that the write of a sign-out can never be
  // overtaken by an older write of a refresh that was still on its way.
  let writes: Promise<void> = Promise.resolve();
  const save = (value: TokenSet | null): Promise<void> => {
    const next = writes.catch(() => undefined).then(() => storage.saveTokens(value));
    writes = next;
    return next;
  };

  const change = (next: SessionStatus) => {
    const differs = next !== status;
    status = next;
    if (differs) for (const l of listeners) l();
  };

  /** The sign-in is over on this phone (not by the driver's choice): the tokens go, the trips stay. */
  async function end(by: EndedBy): Promise<null> {
    generation += 1;
    tokens = null;
    ended = by;
    await save(null);
    change("expired");
    return null;
  }

  async function refresh(): Promise<string | null> {
    if (!config || !tokens) return null;
    const { baseUrl, clientId } = config;
    const spent = tokens.refreshToken;
    const mine = generation;
    try {
      const next = await refreshTokens(oauth, {
        baseUrl,
        clientId,
        refreshToken: spent,
        now: now(),
        timeoutMs: refreshTimeoutMs,
      });
      // The driver signed out, or signed in again, while the instance was answering: this pair
      // belongs to a sign-in that is over. It is neither kept nor written. (The instance has
      // rotated the token; the sign-out told it to end the whole sign-in.)
      if (mine !== generation) return null;
      // The refresh token is spent: the new pair is kept in memory first, so that a failed write
      // never leaves the spent one in use (the instance ends the whole sign-in when a refresh
      // token is presented twice), and then written, twice if need be.
      tokens = next;
      ended = null;
      change("signed_in");
      try {
        await save(next);
      } catch {
        if (mine !== generation) return null;
        try {
          await save(next);
        } catch {
          // The Keystore still holds the spent pair: the next start of the app will find the
          // sign-in ended and ask for a new one. Nothing else can be done here.
        }
      }
      return next.accessToken;
    } catch (error) {
      if (mine !== generation) return null;
      if (error instanceof OAuthError && ENDED.includes(error.code)) {
        // The instance ended this sign-in. Only the tokens go; the trips on the phone stay.
        // Defence in depth for other servers: the obop server never answers `invalid_client` to a
        // refresh (an unknown client or token is `invalid_grant`, `exchange_refresh`), so for it
        // this branch is not reached. Where it is, the client is registered again at the next
        // sign-in. The real recovery from a client the instance forgot is the probe of the
        // authorization page before the browser opens (`clientUnknown`, docs/ISSUES.md L40).
        if (error.code === "invalid_client" || error.code === "unauthorized_client") {
          config = { ...config, usedAt: undefined };
          await storage.saveConfig(config).catch(() => undefined);
          // A sign-in finished while the config was written: it is not ended by this answer.
          if (mine !== generation) return null;
        }
        return end("refused");
      }
      if (error instanceof OAuthError && error.timedOut) {
        // No answer in time: the instance may have rotated the token and the answer is lost. The
        // old token is spent or soon will be, and the instance ends the whole sign-in when it is
        // presented again, so it is not presented again: the sign-in ends here, cleanly, and the
        // instance is told in the background to end it too (the old token still finds the grant).
        const failed = revokeToken(oauth, {
          baseUrl,
          clientId,
          token: spent,
          timeoutMs: END_REVOKE_MS,
        }).then(
          () => false,
          () => true,
        );
        const ending = end("unsure"); // moves the generation at once
        const endedAt = generation;
        // When the revoke fails too the instance still lists the phone and nothing else will tell
        // the driver: the sign-in screen says so (the same notice as after a sign-out that could
        // not reach the instance), unless a newer sign-in has finished by then.
        void failed.then((untold) => {
          if (untold && generation === endedAt) onEndUntold?.();
        });
        return ending;
      }
      throw error;
    }
  }

  return {
    status: () => status,
    endedBy: () => (status === "expired" ? ended : null),
    generation: () => generation,
    config: () => config,
    deviceId: () => tokens?.deviceId ?? null,
    refreshToken: () => tokens?.refreshToken ?? null,

    async accessToken() {
      if (!tokens) return null;
      if (tokens.expiresAt - now() > skewMs) return tokens.accessToken;
      // Calls that meet an old token at the same moment share one refresh: a second use of the
      // same refresh token would end the whole sign-in.
      if (refreshing === null) {
        const run: Promise<string | null> = refresh().finally(() => {
          // A refresh of an earlier sign-in must not clear the one of the current sign-in.
          if (refreshing === run) refreshing = null;
        });
        refreshing = run;
      }
      return refreshing;
    },

    async rejected() {
      if (tokens) tokens = { ...tokens, expiresAt: 0 };
    },

    async restore() {
      config = await storage.loadConfig();
      tokens = await storage.loadTokens();
      generation += 1;
      ended = null;
      status = tokens && config ? "signed_in" : "signed_out";
      for (const l of listeners) l();
    },

    async setConfig(next) {
      await storage.saveConfig(next);
      config = next;
    },

    async completeSignIn(next) {
      // Anything still waiting for the instance under the earlier sign-in is dropped from here on.
      generation += 1;
      ended = null;
      await save(next);
      tokens = next;
      change("signed_in");
    },

    async signOut() {
      // First, so that a refresh whose answer is still on its way finds the sign-in gone and
      // neither keeps its tokens nor writes them: a signed-out phone must not end up signed in.
      generation += 1;
      tokens = null;
      ended = null;
      refreshing = null;
      await save(null);
      change("signed_out");
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/**
 * The sign-in of a native app (RFC 8252, RFC 7636): the pieces that are the same for every app.
 * Dynamic registration of a client of the `mobile` class, a PKCE pair, the authorization address
 * to open in the system browser, the reading of the redirect, and the exchange and refresh of
 * tokens at `POST /oauth/token` (docs/DEPLOY.md "Mobile apps").
 *
 * Nothing here touches a platform: the app injects the hash (`sha256`), the random source
 * (`randomBytes`) and, in `auth.ts`, the storage of the tokens, so that the same code runs in the
 * tests and on the phone. No token or code is ever put into an error message.
 */

export interface OAuthDeps {
  /** SHA-256 of the UTF-8 bytes of `text`. */
  sha256: (text: string) => Promise<Uint8Array>;
  /** `count` bytes from a cryptographically secure random source. */
  randomBytes: (count: number) => Promise<Uint8Array>;
  /** The `fetch` to use; the global one when absent. */
  fetch?: typeof fetch;
  /**
   * Milliseconds after which a call to the instance is given up (`network_error`). React Native's
   * network stack has no timeout of its own, so without this a half-open connection stalls for
   * minutes. 30 seconds when absent. A refresh that times out may have been answered: see the
   * single-use rule in the notes of `refreshTokens`.
   */
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 30_000;

/** A refusal by the authorization server, or an answer that is not what it sends. */
export class OAuthError extends Error {
  /** The OAuth error word (`invalid_grant`, `invalid_client`, ...), or one of ours (`network_error`, `invalid_response`, `state_mismatch`, `access_denied`). */
  readonly code: string;
  /** The HTTP status, 0 when there was no answer. */
  readonly status: number;

  constructor(code: string, message: string, status = 0, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "OAuthError";
    this.code = code;
    this.status = status;
  }
}

/** The tokens of one sign-in. `expiresAt` is a time in milliseconds since the Unix epoch. */
export interface TokenSet {
  accessToken: string;
  refreshToken: string;
  expiresAt: number;
  /** The `device` record of this sign-in (a mobile app is told which device it is). */
  deviceId: string | null;
  /** The groups the access token holds. */
  scope: string[];
}

/** What a mobile app tells the instance about itself at the exchange of the code (D64, `device`). */
export interface DeviceInfo {
  name: string;
  platform: string;
  appVersion: string;
}

const BASE64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** Base64 with the URL alphabet and no padding (RFC 4648 section 5), as PKCE and `state` want it. */
export function base64Url(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i]!;
    const b1 = bytes[i + 1];
    const b2 = bytes[i + 2];
    out += BASE64URL[b0 >> 2]!;
    out += BASE64URL[((b0 & 3) << 4) | ((b1 ?? 0) >> 4)]!;
    if (b1 !== undefined) out += BASE64URL[((b1 & 15) << 2) | ((b2 ?? 0) >> 6)]!;
    if (b2 !== undefined) out += BASE64URL[b2 & 63]!;
  }
  return out;
}

export interface Pkce {
  /** Kept on the device until the code is exchanged; never sent to the browser. */
  verifier: string;
  /** `BASE64URL(SHA256(verifier))`, sent in the authorization address. */
  challenge: string;
  method: "S256";
}

/** A PKCE pair: a verifier of 43 characters (32 random bytes) and its S256 challenge. */
export async function createPkce(deps: Pick<OAuthDeps, "sha256" | "randomBytes">): Promise<Pkce> {
  const verifier = base64Url(await deps.randomBytes(32));
  const challenge = base64Url(await deps.sha256(verifier));
  return { verifier, challenge, method: "S256" };
}

/** A random `state` for the authorization request (CSRF protection of the redirect). */
export async function createState(deps: Pick<OAuthDeps, "randomBytes">): Promise<string> {
  return base64Url(await deps.randomBytes(16));
}

/** The address of an instance without a trailing slash. */
export function normaliseBaseUrl(address: string): string {
  return address.trim().replace(/\/+$/, "");
}

function formBody(fields: Record<string, string | undefined>): string {
  return Object.entries(fields)
    .filter((entry): entry is [string, string] => entry[1] !== undefined)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join("&");
}

async function post(
  deps: OAuthDeps,
  url: string,
  init: { headers: Record<string, string>; body: string },
  timeoutMs: number = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS,
): Promise<Record<string, unknown>> {
  let response: Response;
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), timeoutMs);
  // The signal stops the request itself; the race covers a fetch that does not honour it.
  const stalled = new Promise<never>((_, reject) => {
    abort.signal.addEventListener("abort", () =>
      reject(new OAuthError("network_error", "The server did not answer in time")),
    );
  });
  stalled.catch(() => undefined);
  try {
    response = await Promise.race([
      (deps.fetch ?? globalThis.fetch)(url, { method: "POST", ...init, signal: abort.signal }),
      stalled,
    ]);
  } catch (cause) {
    clearTimeout(timer);
    if (cause instanceof OAuthError) throw cause;
    throw new OAuthError("network_error", "Could not reach the server", 0, { cause });
  }
  clearTimeout(timer);
  const text = await response.text().catch(() => "");
  let body: Record<string, unknown> | undefined;
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)) {
      body = parsed as Record<string, unknown>;
    }
  } catch {
    // Not JSON: handled below.
  }
  if (!response.ok) {
    const code = typeof body?.error === "string" ? body.error : "server_error";
    const description =
      typeof body?.error_description === "string" ? body.error_description : response.statusText;
    throw new OAuthError(
      code,
      description || `The server answered ${response.status}`,
      response.status,
    );
  }
  if (!body) {
    throw new OAuthError(
      "invalid_response",
      "The server did not answer like an obop instance. Is the address right?",
      response.status,
    );
  }
  return body;
}

export interface RegisterOptions {
  baseUrl: string;
  /** A private-use redirect of the form `org.obop.mileage:/oauth` (RFC 8252 section 7.1). */
  redirectUri: string;
  name: string;
  /** The groups the app will ask for; a phone is offered only what it declared here. */
  scope: readonly string[];
}

/** `POST /oauth/register` for a client of the `mobile` class (RFC 7591). Answers the `client_id`. */
export async function registerMobileClient(
  deps: OAuthDeps,
  options: RegisterOptions,
): Promise<string> {
  const body = await post(deps, `${normaliseBaseUrl(options.baseUrl)}/oauth/register`, {
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      client_name: options.name,
      redirect_uris: [options.redirectUri],
      grant_types: ["authorization_code", "refresh_token"],
      response_types: ["code"],
      token_endpoint_auth_method: "none",
      obop_client_class: "mobile",
      scope: options.scope.join(" "),
    }),
  });
  if (typeof body.client_id !== "string" || body.client_id === "") {
    throw new OAuthError("invalid_response", "The registration answered no client id");
  }
  return body.client_id;
}

export interface AuthorizeOptions {
  baseUrl: string;
  clientId: string;
  redirectUri: string;
  challenge: string;
  state: string;
  /** The groups to tick on the consent page; they must be among those declared at registration. */
  scope?: readonly string[];
}

/** The address to open in the system browser: `GET /oauth/authorize` with PKCE S256. */
export function buildAuthorizeUrl(options: AuthorizeOptions): string {
  const query = formBody({
    response_type: "code",
    client_id: options.clientId,
    redirect_uri: options.redirectUri,
    code_challenge: options.challenge,
    code_challenge_method: "S256",
    state: options.state,
    scope: options.scope && options.scope.length > 0 ? options.scope.join(" ") : undefined,
  });
  return `${normaliseBaseUrl(options.baseUrl)}/oauth/authorize?${query}`;
}

function queryOf(url: string): Record<string, string> {
  const hashAt = url.indexOf("#");
  const clean = hashAt < 0 ? url : url.slice(0, hashAt);
  const at = clean.indexOf("?");
  const out: Record<string, string> = {};
  if (at < 0) return out;
  for (const pair of clean.slice(at + 1).split("&")) {
    if (pair === "") continue;
    const eq = pair.indexOf("=");
    const raw = (s: string) => {
      try {
        return decodeURIComponent(s.replace(/\+/g, " "));
      } catch {
        return s;
      }
    };
    const key = raw(eq < 0 ? pair : pair.slice(0, eq));
    if (!(key in out)) out[key] = eq < 0 ? "" : raw(pair.slice(eq + 1));
  }
  return out;
}

/**
 * The code in the redirect the browser handed back. Refuses a redirect whose `state` is not the one
 * that was sent, whose `iss` (RFC 9207), when there is one, is not this instance, and a redirect
 * that carries an `error` (the person said no: `access_denied`).
 */
export function parseAuthorizationResponse(
  redirectUrl: string,
  expected: { state: string; baseUrl: string },
): string {
  const q = queryOf(redirectUrl);
  // The state is checked first: any app can open the redirect address, and an `error` in a link
  // that does not carry this sign-in's state must not end the sign-in that is waiting.
  if (q.state !== expected.state) {
    throw new OAuthError("state_mismatch", "The answer did not belong to this sign-in");
  }
  if (q.error) {
    throw new OAuthError(q.error, "The sign-in was not completed", 0);
  }
  if (q.iss !== undefined && normaliseBaseUrl(q.iss) !== normaliseBaseUrl(expected.baseUrl)) {
    throw new OAuthError("issuer_mismatch", "The answer came from another server");
  }
  if (!q.code) throw new OAuthError("invalid_response", "The answer carried no code");
  return q.code;
}

function tokenSet(body: Record<string, unknown>, now: number): TokenSet {
  const access = body.access_token;
  const refresh = body.refresh_token;
  const ttl = body.expires_in;
  if (typeof access !== "string" || typeof refresh !== "string" || typeof ttl !== "number") {
    throw new OAuthError("invalid_response", "The token answer was incomplete");
  }
  return {
    accessToken: access,
    refreshToken: refresh,
    expiresAt: now + ttl * 1000,
    deviceId: typeof body.device_id === "string" ? body.device_id : null,
    scope: typeof body.scope === "string" ? body.scope.split(" ").filter(Boolean) : [],
  };
}

const FORM = { "content-type": "application/x-www-form-urlencoded", accept: "application/json" };

export interface ExchangeOptions {
  baseUrl: string;
  clientId: string;
  redirectUri: string;
  code: string;
  verifier: string;
  device: DeviceInfo;
  now?: number;
}

/** `POST /oauth/token`, `authorization_code`: the tokens of this sign-in and the device record. */
export async function exchangeCode(deps: OAuthDeps, options: ExchangeOptions): Promise<TokenSet> {
  const body = await post(deps, `${normaliseBaseUrl(options.baseUrl)}/oauth/token`, {
    headers: FORM,
    body: formBody({
      grant_type: "authorization_code",
      code: options.code,
      client_id: options.clientId,
      redirect_uri: options.redirectUri,
      code_verifier: options.verifier,
      device_name: options.device.name,
      device_platform: options.device.platform,
      device_app_version: options.device.appVersion,
    }),
  });
  return tokenSet(body, options.now ?? Date.now());
}

export interface RefreshOptions {
  baseUrl: string;
  clientId: string;
  refreshToken: string;
  now?: number;
}

/**
 * `POST /oauth/token`, `refresh_token`. The refresh token rotates: the answer holds the next one,
 * and the one that was sent is spent (a second use of it ends the whole sign-in, RFC 9700), so
 * the caller stores the answer before anything else and never refreshes twice at once.
 */
export async function refreshTokens(deps: OAuthDeps, options: RefreshOptions): Promise<TokenSet> {
  const body = await post(deps, `${normaliseBaseUrl(options.baseUrl)}/oauth/token`, {
    headers: FORM,
    body: formBody({
      grant_type: "refresh_token",
      refresh_token: options.refreshToken,
      client_id: options.clientId,
    }),
  });
  return tokenSet(body, options.now ?? Date.now());
}

export interface RevokeOptions {
  baseUrl: string;
  clientId: string;
  /** The refresh token: revoking it ends the whole sign-in (RFC 7009), and the device with it. */
  token: string;
  /** Gives up after this many milliseconds (the revoke is safe to repeat, so a short wait is fine). */
  timeoutMs?: number;
}

/** `POST /oauth/revoke` (RFC 7009): ends the sign-in on the instance. Answers 200 whatever the token was. */
export async function revokeToken(deps: OAuthDeps, options: RevokeOptions): Promise<void> {
  await post(
    deps,
    `${normaliseBaseUrl(options.baseUrl)}/oauth/revoke`,
    {
      headers: FORM,
      body: formBody({
        token: options.token,
        token_type_hint: "refresh_token",
        client_id: options.clientId,
      }),
    },
    options.timeoutMs,
  );
}

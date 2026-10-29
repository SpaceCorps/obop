import {
  OAuthError,
  ObopNetworkError,
  buildAuthorizeUrl,
  createObopClient,
  createPkce,
  createState,
  exchangeCode,
  parseAuthorizationResponse,
  registerMobileClient,
  revokeToken,
  type TokenSet,
} from "@obop/client";
import Constants from "expo-constants";
import * as WebBrowser from "expo-web-browser";
import { checkAddress as checkAddressText } from "../core/address.ts";
import type { Binding, Owner } from "../core/identity.ts";
import { withDeadline } from "../core/deadline.ts";
import {
  clientUnknown,
  needsRegistration,
  type ServerConfig,
  type Session,
} from "../core/session.ts";
import { CLIENT_NAME, REDIRECT_URI, SCOPE } from "./config.ts";
import { deviceInfo, oauthDeps } from "./platform-deps.ts";

/** Why a sign-in failed, as a word; the catalogue holds the sentence for each. */
export type SignInFailure =
  | "address"
  | "cleartext"
  | "cancelled"
  | "denied"
  | "network"
  | "server"
  | "failed";

export class SignInError extends Error {
  readonly reason: SignInFailure;
  constructor(reason: SignInFailure, cause?: unknown) {
    super(reason, cause === undefined ? undefined : { cause });
    this.name = "SignInError";
    this.reason = reason;
  }
}

/** `http` is allowed only in a build made for a test instance on the same network. */
function cleartextAllowed(): boolean {
  return Constants.expoConfig?.extra?.cleartextAllowed === true;
}

/** The address as the app will use it, or a `SignInError` that says what is wrong with it. */
export function checkAddress(input: string): string {
  const result = checkAddressText(input, cleartextAllowed());
  if (!result.ok) throw new SignInError(result.problem);
  return result.address;
}

function failure(error: unknown): SignInError {
  if (error instanceof SignInError) return error;
  if (error instanceof ObopNetworkError) return new SignInError("network", error);
  if (error instanceof OAuthError) {
    if (error.code === "access_denied") return new SignInError("denied", error);
    if (error.code === "network_error") return new SignInError("network", error);
    if (error.code === "invalid_response") return new SignInError("server", error);
  }
  return new SignInError("failed", error);
}

/** What a sign-in needs from the runtime. */
export interface SignInHost {
  session: Session;
  /** Decides about the trips already on the phone, before anything is uploaded. */
  bindIdentity(owner: Owner): Promise<Binding>;
}

/** How long the question "who is this" and the probe of the authorization page may take. */
const OWNER_CALL_MS = 30_000;
const PROBE_MS = 8000;

/** The person that the instance reports as the owner of the device of this sign-in. */
async function ownerOf(baseUrl: string, tokens: TokenSet): Promise<Owner> {
  const deviceId = tokens.deviceId;
  if (deviceId === null) throw new SignInError("failed");
  const client = createObopClient({
    baseUrl,
    getToken: async () => tokens.accessToken,
    getLanguage: () => "en",
  });
  // Bounded: React Native's network stack has no timeout of its own (docs/ISSUES.md L29).
  const { value } = await withDeadline(OWNER_CALL_MS, (signal) =>
    client.call<{ user?: unknown }>("device", "get", { id: deviceId }, { signal }),
  );
  if (typeof value.user !== "string" || value.user === "") throw new SignInError("failed");
  return { origin: baseUrl, user: value.user };
}

/**
 * The status that the authorization page answers for this address, or `null` when it did not
 * answer (any failure: the sign-in goes on and the browser shows what is wrong). The page is read,
 * not followed, and the instance keeps a sign-in flow for it that runs out by itself.
 */
async function probeAuthorize(url: string): Promise<number | null> {
  try {
    return await withDeadline(PROBE_MS, async (signal) => {
      const response = await fetch(url, { signal, redirect: "manual" });
      return response.status;
    });
  } catch {
    return null;
  }
}

/**
 * Signs in with the system browser (Custom Tabs; never a web view in the app): registers the app
 * with the instance (again at every sign-in until one has finished with the client, because the
 * instance forgets a client that no sign-in used), opens the authorization page with PKCE S256,
 * reads the redirect, and exchanges the code for tokens. Before the tokens are used it asks the
 * instance who the person is and decides about the trips on the phone (`bindIdentity`): trips of
 * another sign-in are neither uploaded nor shown. The tokens go to the Keystore through the session.
 */
export async function signIn(host: SignInHost, input: string): Promise<void> {
  const { session } = host;
  try {
    const baseUrl = checkAddress(input);
    const register = async (): Promise<ServerConfig> => {
      const clientId = await registerMobileClient(oauthDeps, {
        baseUrl,
        redirectUri: REDIRECT_URI,
        name: CLIENT_NAME,
        scope: SCOPE,
      });
      const fresh = { baseUrl, clientId };
      await session.setConfig(fresh);
      return fresh;
    };
    let config = session.config();
    let registeredNow = false;
    if (needsRegistration(config, baseUrl, Date.now())) {
      config = await register();
      registeredNow = true;
    }
    if (config === null) throw new SignInError("failed");
    const pkce = await createPkce(oauthDeps);
    const state = await createState(oauthDeps);
    const authorizeUrl = (clientId: string) =>
      buildAuthorizeUrl({
        baseUrl,
        clientId,
        redirectUri: REDIRECT_URI,
        challenge: pkce.challenge,
        state,
        scope: SCOPE,
      });
    let url = authorizeUrl(config.clientId);
    // A client that is used again may have been forgotten by the instance for a reason other than
    // idleness (a replaced database, a restore): the page then answers 400 and the browser shows
    // an error that the app cannot read. Register once more and go on (docs/ISSUES.md L40).
    if (!registeredNow && clientUnknown(await probeAuthorize(url))) {
      config = await register();
      url = authorizeUrl(config.clientId);
    }
    const result = await WebBrowser.openAuthSessionAsync(url, REDIRECT_URI);
    if (result.type !== "success") throw new SignInError("cancelled");
    const code = parseAuthorizationResponse(result.url, { state, baseUrl });
    const tokens = await exchangeCode(oauthDeps, {
      baseUrl,
      clientId: config.clientId,
      redirectUri: REDIRECT_URI,
      code,
      verifier: pkce.verifier,
      device: deviceInfo(),
    });
    try {
      const owner = await ownerOf(baseUrl, tokens);
      await host.bindIdentity(owner);
      await session.setConfig({ ...config, user: owner.user, usedAt: Date.now() });
    } catch (error) {
      // The sign-in is not finished: do not leave it active on the instance (best effort).
      await revokeToken(oauthDeps, {
        baseUrl,
        clientId: config.clientId,
        token: tokens.refreshToken,
        timeoutMs: 5000,
      }).catch(() => undefined);
      throw error;
    }
    await session.completeSignIn(tokens);
  } catch (error) {
    throw failure(error);
  }
}

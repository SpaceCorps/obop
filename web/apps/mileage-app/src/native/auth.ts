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
import { needsRegistration, type Session } from "../core/session.ts";
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

/** The person that the instance reports as the owner of the device of this sign-in. */
async function ownerOf(baseUrl: string, tokens: TokenSet): Promise<Owner> {
  if (tokens.deviceId === null) throw new SignInError("failed");
  const client = createObopClient({
    baseUrl,
    getToken: async () => tokens.accessToken,
    getLanguage: () => "en",
  });
  const { value } = await client.call<{ user?: unknown }>("device", "get", {
    id: tokens.deviceId,
  });
  if (typeof value.user !== "string" || value.user === "") throw new SignInError("failed");
  return { origin: baseUrl, user: value.user };
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
    let config = session.config();
    if (needsRegistration(config, baseUrl, Date.now())) {
      const clientId = await registerMobileClient(oauthDeps, {
        baseUrl,
        redirectUri: REDIRECT_URI,
        name: CLIENT_NAME,
        scope: SCOPE,
      });
      config = { baseUrl, clientId };
      await session.setConfig(config);
    }
    if (config === null) throw new SignInError("failed");
    const pkce = await createPkce(oauthDeps);
    const state = await createState(oauthDeps);
    const url = buildAuthorizeUrl({
      baseUrl,
      clientId: config.clientId,
      redirectUri: REDIRECT_URI,
      challenge: pkce.challenge,
      state,
      scope: SCOPE,
    });
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

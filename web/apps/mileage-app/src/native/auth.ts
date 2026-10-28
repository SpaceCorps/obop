import {
  OAuthError,
  buildAuthorizeUrl,
  createPkce,
  createState,
  exchangeCode,
  normaliseBaseUrl,
  parseAuthorizationResponse,
  registerMobileClient,
} from "@obop/client";
import Constants from "expo-constants";
import * as WebBrowser from "expo-web-browser";
import type { Session } from "../core/session.ts";
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
  const address = normaliseBaseUrl(input);
  let url: URL;
  try {
    url = new URL(address);
  } catch {
    throw new SignInError("address");
  }
  if (url.protocol === "https:") return address;
  if (url.protocol === "http:") {
    if (cleartextAllowed()) return address;
    throw new SignInError("cleartext");
  }
  throw new SignInError("address");
}

function failure(error: unknown): SignInError {
  if (error instanceof SignInError) return error;
  if (error instanceof OAuthError) {
    if (error.code === "access_denied") return new SignInError("denied", error);
    if (error.code === "network_error") return new SignInError("network", error);
    if (error.code === "invalid_response") return new SignInError("server", error);
  }
  return new SignInError("failed", error);
}

/**
 * Signs in with the system browser (Custom Tabs; never a web view in the app): registers the app
 * with the instance once per address, opens the authorization page with PKCE S256, reads the
 * redirect, and exchanges the code for tokens, which go to the Keystore through the session.
 */
export async function signIn(session: Session, input: string): Promise<void> {
  try {
    const baseUrl = checkAddress(input);
    let config = session.config();
    if (config?.baseUrl !== baseUrl) {
      const clientId = await registerMobileClient(oauthDeps, {
        baseUrl,
        redirectUri: REDIRECT_URI,
        name: CLIENT_NAME,
        scope: SCOPE,
      });
      config = { baseUrl, clientId };
      await session.setConfig(config);
    }
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
    await session.completeSignIn(tokens);
  } catch (error) {
    throw failure(error);
  }
}

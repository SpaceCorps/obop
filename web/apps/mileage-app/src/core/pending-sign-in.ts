import type { ServerConfig } from "./session.ts";

/**
 * A sign-in that waits for the browser: what the app needs to finish it when the redirect arrives.
 * It is written to the Keystore or the Keychain before the browser opens, because the system may
 * end the app while the browser is in front (a password manager, an authenticator app for the
 * second factor, low memory), and the redirect then starts the app with nothing waiting for it
 * (docs/ISSUES.md L32). It is removed when the sign-in ends, however it ends.
 *
 * The verifier is secret, but only for the time of one sign-in: the code it fits is single use,
 * bound to this client and this redirect, and runs out 60 seconds after the redirect.
 */
export interface PendingSignIn {
  /** The instance and the client the sign-in was begun with. */
  config: ServerConfig;
  /** The PKCE verifier of the challenge in the address the browser opened. */
  verifier: string;
  /** The `state` of that address. */
  state: string;
  /** When the browser was opened (ms). */
  startedAt: number;
}

/**
 * How long a pending sign-in can still be finished: the instance keeps its sign-in flow for 10
 * minutes (`flow_ttl_secs`), and a redirect cannot come later than that.
 */
export const PENDING_SIGN_IN_MS = 10 * 60 * 1000;

const isConfig = (v: unknown): v is ServerConfig =>
  typeof v === "object" &&
  v !== null &&
  typeof (v as ServerConfig).baseUrl === "string" &&
  typeof (v as ServerConfig).clientId === "string";

/** Whether a stored value is a pending sign-in (anything else is ignored and removed). */
export function isPendingSignIn(value: unknown): value is PendingSignIn {
  if (typeof value !== "object" || value === null) return false;
  const p = value as PendingSignIn;
  return (
    isConfig(p.config) &&
    typeof p.verifier === "string" &&
    p.verifier !== "" &&
    typeof p.state === "string" &&
    p.state !== "" &&
    typeof p.startedAt === "number" &&
    Number.isFinite(p.startedAt)
  );
}

/**
 * Whether the address that started the app is the redirect of this pending sign-in, so that the
 * sign-in can be finished: the address is the app's redirect (and nothing that only begins like it),
 * it carries the same `state`, and the sign-in was begun less than `PENDING_SIGN_IN_MS` ago (and not
 * in the future: a clock that went back does not make an old one fresh). Anything else is not
 * finished, and the pending sign-in is dropped.
 */
export function resumable(
  pending: PendingSignIn | null,
  url: string | null,
  redirectUri: string,
  nowMs: number,
): boolean {
  if (pending === null || url === null) return false;
  const age = nowMs - pending.startedAt;
  if (!(age >= 0 && age <= PENDING_SIGN_IN_MS)) return false;
  if (!url.startsWith(redirectUri)) return false;
  const rest = url.slice(redirectUri.length);
  if (rest !== "" && !rest.startsWith("?") && !rest.startsWith("#")) return false;
  const query = rest.startsWith("?") ? rest.slice(1).split("#")[0]! : "";
  return new URLSearchParams(query).get("state") === pending.state;
}

/* Passkeys (WebAuthn, D65): the browser half. The server gives options as JSON with every binary
   field base64url encoded; this file turns them into what `navigator.credentials` wants, runs the
   ceremony, and turns the answer back into JSON for the server. Nothing here decides anything:
   the origin, the relying party and every check are the server's. */

export type Reauth = { password: string; code?: string; recovery_code?: string };

export type Passkey = {
  id: string;
  name: string;
  created_at: string | null;
  last_used_at: string | null;
  user_verified: boolean;
  discoverable: boolean | null;
  backed_up: boolean;
  transports: string[];
};

export type PasskeyList = {
  available: boolean;
  unavailable_reason: string | null;
  rp_id: string;
  passkeys: Passkey[];
  max: number;
  passwordless: boolean;
  passwordless_possible: boolean;
  totp: boolean;
  policy_requires_second_factor: boolean;
};

export type Failure = {
  ok: false;
  /** `cancelled`: the person dismissed the browser's prompt or it timed out. */
  kind: "cancelled" | "duplicate" | "unsupported" | "server" | "network";
  status: number;
  code: string;
  message: string;
};
export type Outcome<T> = { ok: true; value: T } | Failure;

export function passkeysSupported(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.PublicKeyCredential === "function" &&
    !!navigator.credentials
  );
}

// ---- base64url -----------------------------------------------------------------------------

export function toBuffer(s: string): ArrayBuffer {
  const b = atob(
    s
      .replace(/-/g, "+")
      .replace(/_/g, "/")
      .padEnd(Math.ceil(s.length / 4) * 4, "="),
  );
  const out = new Uint8Array(b.length);
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i);
  return out.buffer;
}

export function toBase64Url(b: ArrayBuffer): string {
  let s = "";
  for (const x of new Uint8Array(b)) s += String.fromCharCode(x);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

type Descriptor = { type: "public-key"; id: string; transports?: AuthenticatorTransport[] };

function descriptors(list: Descriptor[] | undefined): PublicKeyCredentialDescriptor[] {
  return (list ?? []).map((d) => ({ ...d, id: toBuffer(d.id) }));
}

type CreationJson = {
  publicKey: Omit<
    PublicKeyCredentialCreationOptions,
    "challenge" | "user" | "excludeCredentials"
  > & {
    challenge: string;
    user: { id: string; name: string; displayName: string };
    excludeCredentials?: Descriptor[];
  };
};
type RequestJson = {
  publicKey: Omit<PublicKeyCredentialRequestOptions, "challenge" | "allowCredentials"> & {
    challenge: string;
    allowCredentials?: Descriptor[];
  };
};

export function creationOptions(o: CreationJson): CredentialCreationOptions {
  const p = o.publicKey;
  return {
    publicKey: {
      ...p,
      challenge: toBuffer(p.challenge),
      user: { ...p.user, id: toBuffer(p.user.id) },
      excludeCredentials: descriptors(p.excludeCredentials),
    },
  };
}

export function requestOptions(o: RequestJson): CredentialRequestOptions {
  const p = o.publicKey;
  return {
    publicKey: {
      ...p,
      challenge: toBuffer(p.challenge),
      allowCredentials: descriptors(p.allowCredentials),
    },
  };
}

/** A new credential as the JSON the server expects. */
export function creationJson(c: PublicKeyCredential) {
  const r = c.response as AuthenticatorAttestationResponse;
  return {
    id: c.id,
    rawId: toBase64Url(c.rawId),
    type: c.type,
    clientExtensionResults: c.getClientExtensionResults(),
    response: {
      clientDataJSON: toBase64Url(r.clientDataJSON),
      attestationObject: toBase64Url(r.attestationObject),
      transports: typeof r.getTransports === "function" ? r.getTransports() : [],
    },
  };
}

/** An assertion as the JSON the server expects. */
export function assertionJson(c: PublicKeyCredential) {
  const r = c.response as AuthenticatorAssertionResponse;
  return {
    id: c.id,
    rawId: toBase64Url(c.rawId),
    type: c.type,
    clientExtensionResults: c.getClientExtensionResults(),
    response: {
      clientDataJSON: toBase64Url(r.clientDataJSON),
      authenticatorData: toBase64Url(r.authenticatorData),
      signature: toBase64Url(r.signature),
      userHandle: r.userHandle ? toBase64Url(r.userHandle) : null,
    },
  };
}

// ---- calls -----------------------------------------------------------------------------------

async function call<T>(
  method: "GET" | "POST",
  path: string,
  token: string | null,
  body?: object,
): Promise<Outcome<T>> {
  try {
    const res = await fetch(path, {
      method,
      headers: {
        "content-type": "application/json",
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const json = (await res.json().catch(() => ({}))) as { code?: string; error?: string };
    if (res.ok) return { ok: true, value: json as T };
    return {
      ok: false,
      kind: "server",
      status: res.status,
      code: json.code ?? "",
      message: json.error ?? "",
    };
  } catch {
    return { ok: false, kind: "network", status: 0, code: "network", message: "" };
  }
}

/** What the browser said when a ceremony did not complete. */
function browserFailure(e: unknown): Failure {
  const name = e instanceof DOMException ? e.name : "";
  const kind =
    name === "NotAllowedError" || name === "AbortError"
      ? "cancelled"
      : name === "InvalidStateError"
        ? "duplicate"
        : name === "NotSupportedError"
          ? "unsupported"
          : "server";
  return { ok: false, kind, status: 0, code: name, message: e instanceof Error ? e.message : "" };
}

async function ceremony<T>(run: () => Promise<T>): Promise<Outcome<T>> {
  try {
    return { ok: true, value: await run() };
  } catch (e) {
    return browserFailure(e);
  }
}

export function listPasskeys(token: string) {
  return call<PasskeyList>("GET", "/api/auth/passkeys", token);
}

export type Registered = { id: string; name: string; recovery_codes?: string[] };

/** Add a passkey: the server checks the password (and second factor), the browser makes the key. */
export async function registerPasskey(
  token: string,
  reauth: Reauth,
  name: string,
): Promise<Outcome<Registered>> {
  const opts = await call<CreationJson>(
    "POST",
    "/api/auth/passkeys/register/options",
    token,
    reauth,
  );
  if (!opts.ok) return opts;
  const made = await ceremony(() => navigator.credentials.create(creationOptions(opts.value)));
  if (!made.ok) return made;
  if (!made.value) return browserFailure(new DOMException("", "NotAllowedError"));
  return call<Registered>("POST", "/api/auth/passkeys/register/finish", token, {
    name,
    credential: creationJson(made.value as PublicKeyCredential),
  });
}

/** Prove possession of a passkey in this session: the next re-authentication needs no code. */
export async function proveWithPasskey(token: string): Promise<Outcome<true>> {
  const opts = await call<RequestJson>("POST", "/api/auth/passkeys/proof/options", token, {});
  if (!opts.ok) return opts;
  const got = await ceremony(() => navigator.credentials.get(requestOptions(opts.value)));
  if (!got.ok) return got;
  if (!got.value) return browserFailure(new DOMException("", "NotAllowedError"));
  const r = await call<unknown>("POST", "/api/auth/passkeys/proof", token, {
    credential: assertionJson(got.value as PublicKeyCredential),
  });
  return r.ok ? { ok: true, value: true } : r;
}

export function renamePasskey(token: string, id: string, name: string, reauth: Reauth) {
  return call<{ renamed: true }>("POST", `/api/auth/passkeys/${id}/rename`, token, {
    name,
    ...reauth,
  });
}

export function removePasskey(token: string, id: string, reauth: Reauth) {
  return call<{ removed: true }>("POST", `/api/auth/passkeys/${id}/remove`, token, reauth);
}

export function setPasswordless(token: string, enabled: boolean, reauth: Reauth) {
  return call<{ passwordless: boolean }>("POST", "/api/auth/passkeys/passwordless", token, {
    enabled,
    ...reauth,
  });
}

export type SignedIn = {
  token: string;
  user?: string;
  mfa_enrolment_required?: boolean;
};

/** Sign in with a passkey: the second step of a pending sign-in, or on its own (usernameless). */
export async function signInWithPasskey(mfaToken?: string): Promise<Outcome<SignedIn>> {
  const opts = await call<RequestJson>(
    "POST",
    "/api/auth/login/passkey/options",
    null,
    mfaToken ? { mfa_token: mfaToken } : {},
  );
  if (!opts.ok) return opts;
  const got = await ceremony(() => navigator.credentials.get(requestOptions(opts.value)));
  if (!got.ok) return got;
  if (!got.value) return browserFailure(new DOMException("", "NotAllowedError"));
  return call<SignedIn>("POST", "/api/auth/login/passkey", null, {
    ...(mfaToken ? { mfa_token: mfaToken } : {}),
    credential: assertionJson(got.value as PublicKeyCredential),
  });
}

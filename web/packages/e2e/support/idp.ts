// A stand-in for Microsoft Entra, for the Microsoft sign-in specs (D91): the discovery document, the
// key set, a page where "the person" picks who signs in (or says no), and the token endpoint with
// PKCE and the client secret checked. It signs id tokens with a key it makes when it starts.
//
// It is not Entra and proves nothing about Entra: it speaks the protocol as the decision record
// reads it, and it hands out the claims the way Microsoft's documentation says Entra does: the
// default claims always (`iss`, `tid`, `aud`, `sub`, `nonce`, times); `oid`, `preferred_username`
// and `name` only when the `profile` scope was asked for and is consented to; `amr` and `auth_time`
// only when the app registration has them as optional claims. `GET /control?optional_claims=0|1`
// and `consent=0|1` change what the registration is like, and `GET /control/scopes` lists the
// scopes the authorization requests asked for. Nothing here reaches a network beyond loopback.
// Test code only.
import { createHash, createPrivateKey, generateKeyPairSync, randomBytes, sign } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { sso } from "./env.ts";

const KID = "e2e-idp-key-1";

const b64url = (b: Buffer | string) => Buffer.from(b).toString("base64url");

function page(res: ServerResponse, status: number, body: string) {
  res.statusCode = status;
  res.setHeader("content-type", "text/html; charset=utf-8");
  res.setHeader("cache-control", "no-store");
  res.end(
    `<!doctype html><meta charset="utf-8"><title>Stand-in for Microsoft sign-in</title><body style="font:16px system-ui;max-width:32rem;margin:10vh auto"><h1>Stand-in for Microsoft sign-in</h1>${body}`,
  );
}

function json(res: ServerResponse, status: number, v: unknown) {
  res.statusCode = status;
  res.setHeader("content-type", "application/json");
  res.setHeader("cache-control", "no-store");
  res.end(JSON.stringify(v));
}

async function readBody(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const c of req) chunks.push(c as Buffer);
  return Buffer.concat(chunks).toString("utf8");
}

type Flow = {
  clientId: string;
  redirectUri: string;
  state: string;
  nonce: string;
  challenge: string;
  scope: string;
};
type Issued = Flow & { oid: string; mfa: boolean };

export function startIdp(port: number) {
  const authority = `http://127.0.0.1:${port}`;
  const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const jwk = publicKey.export({ format: "jwk" }) as { n: string; e: string };
  const key = createPrivateKey(privateKey.export({ format: "pem", type: "pkcs8" }));
  const flows = new Map<string, Flow>();
  const codes = new Map<string, Issued>();
  const t = sso.tenant;
  // What the app registration is like: the optional claims added to the ID token, and the consent
  // for the permissions of the `profile` scope.
  const registration = { optionalClaims: true, consent: true };
  const asked: string[] = [];

  /** The scope Entra grants for what was asked: `profile` only where it is consented to. */
  const granted = (scope: string) =>
    scope
      .split(" ")
      .filter((s) => s && (s !== "profile" || registration.consent))
      .join(" ");

  const idToken = (i: Issued): string => {
    const now = Math.floor(Date.now() / 1000);
    const header = { alg: "RS256", typ: "JWT", kid: KID };
    const profile = granted(i.scope).split(" ").includes("profile");
    const claims: Record<string, unknown> = {
      iss: `${authority}/${t}/v2.0`,
      tid: t,
      aud: i.clientId,
      sub: `pairwise-${i.oid}`,
      nonce: i.nonce,
      ver: "2.0",
      iat: now,
      nbf: now,
      exp: now + 3600,
      // The claims of the `profile` scope: the object id the binding is made on, and two that the
      // server reads and keeps nowhere.
      ...(profile
        ? {
            oid: i.oid,
            preferred_username: `${i.oid.slice(0, 8)}@example.test`,
            name: "Stand-in Person",
          }
        : {}),
      // The optional claims of the registration.
      ...(registration.optionalClaims
        ? { auth_time: now, amr: i.mfa ? ["pwd", "mfa"] : ["pwd"] }
        : {}),
    };
    const input = `${b64url(JSON.stringify(header))}.${b64url(JSON.stringify(claims))}`;
    return `${input}.${b64url(sign("RSA-SHA256", Buffer.from(input), key))}`;
  };

  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", authority);
    const path = url.pathname;
    try {
      if (path === "/health") return json(res, 200, { ok: true });
      // The registration of the stand-in, for the specs.
      if (path === "/control") {
        const q = url.searchParams;
        if (q.has("optional_claims"))
          registration.optionalClaims = q.get("optional_claims") === "1";
        if (q.has("consent")) registration.consent = q.get("consent") === "1";
        return json(res, 200, registration);
      }
      if (path === "/control/scopes") return json(res, 200, asked);
      // Browsers ask for an icon; an empty answer keeps the console guard of the specs quiet.
      if (path === "/favicon.ico") {
        res.statusCode = 204;
        return void res.end();
      }
      if (req.method === "GET" && path === `/${t}/v2.0/.well-known/openid-configuration`)
        return json(res, 200, {
          issuer: `${authority}/${t}/v2.0`,
          authorization_endpoint: `${authority}/${t}/oauth2/v2.0/authorize`,
          token_endpoint: `${authority}/${t}/oauth2/v2.0/token`,
          jwks_uri: `${authority}/${t}/discovery/v2.0/keys`,
        });
      if (req.method === "GET" && path === `/${t}/discovery/v2.0/keys`)
        return json(res, 200, {
          keys: [{ kty: "RSA", use: "sig", alg: "RS256", kid: KID, n: jwk.n, e: jwk.e }],
        });

      // The page of "Microsoft": who signs in?
      if (req.method === "GET" && path === `/${t}/oauth2/v2.0/authorize`) {
        const q = url.searchParams;
        const bad =
          q.get("client_id") !== sso.clientId ||
          q.get("response_type") !== "code" ||
          !(q.get("scope") ?? "").split(" ").includes("openid") ||
          (q.get("scope") ?? "").includes("offline_access") ||
          q.get("code_challenge_method") !== "S256" ||
          !q.get("code_challenge") ||
          !q.get("nonce") ||
          !q.get("state") ||
          !q.get("redirect_uri");
        if (bad) return page(res, 400, "<p>Bad authorization request.</p>");
        asked.push(q.get("scope")!);
        const id = randomBytes(12).toString("hex");
        flows.set(id, {
          scope: q.get("scope")!,
          clientId: q.get("client_id")!,
          redirectUri: q.get("redirect_uri")!,
          state: q.get("state")!,
          nonce: q.get("nonce")!,
          challenge: q.get("code_challenge")!,
        });
        const links = Object.keys(sso.identities)
          .map(
            (who) =>
              `<li><a href="/${t}/oauth2/v2.0/pick?flow=${id}&who=${who}">Sign in as ${who}</a></li>`,
          )
          .join("");
        return page(
          res,
          200,
          `<ul>${links}<li><a href="/${t}/oauth2/v2.0/pick?flow=${id}&who=cancel">Cancel</a></li></ul>`,
        );
      }

      // The person chose: back to the client with a code, or with an error.
      if (req.method === "GET" && path === `/${t}/oauth2/v2.0/pick`) {
        const f = flows.get(url.searchParams.get("flow") ?? "");
        const who = url.searchParams.get("who") ?? "";
        if (!f) return page(res, 400, "<p>Unknown flow.</p>");
        flows.delete(url.searchParams.get("flow")!);
        const back = new URL(f.redirectUri);
        if (who === "cancel") {
          back.searchParams.set("error", "access_denied");
          back.searchParams.set(
            "error_description",
            "AADSTS65004: the user declined <b>consent</b>",
          );
        } else {
          const who_ = sso.identities[who as keyof typeof sso.identities];
          if (!who_) return page(res, 400, "<p>Unknown person.</p>");
          const code = `idp-${randomBytes(16).toString("hex")}`;
          codes.set(code, { ...f, oid: who_.oid, mfa: who_.mfa });
          back.searchParams.set("code", code);
        }
        back.searchParams.set("state", f.state);
        res.statusCode = 302;
        res.setHeader("location", back.toString());
        return void res.end();
      }

      if (req.method === "POST" && path === `/${t}/oauth2/v2.0/token`) {
        const f = new URLSearchParams(await readBody(req));
        const i = codes.get(f.get("code") ?? "");
        codes.delete(f.get("code") ?? "");
        const ok =
          i &&
          f.get("grant_type") === "authorization_code" &&
          f.get("client_id") === sso.clientId &&
          f.get("client_secret") === sso.clientSecret &&
          f.get("redirect_uri") === i.redirectUri &&
          createHash("sha256")
            .update(f.get("code_verifier") ?? "")
            .digest("base64url") === i.challenge;
        if (!ok || !i) return json(res, 400, { error: "invalid_grant" });
        return json(res, 200, {
          token_type: "Bearer",
          scope: granted(i.scope),
          expires_in: 3599,
          access_token: "e2e-access-token-the-server-must-not-keep",
          id_token: idToken(i),
        });
      }
      json(res, 404, { error: "not found" });
    } catch (e) {
      json(res, 500, { error: String(e) });
    }
  });
  server.listen(port, "127.0.0.1");
  return server;
}

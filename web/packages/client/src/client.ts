import createClient, { type Client, type Middleware } from "openapi-fetch";
import {
  ObopApiError,
  ObopApprovalRequired,
  ObopNetworkError,
  invalidResponse,
  isReadCall,
  type CallInfo,
} from "./errors.ts";
import type { paths } from "./generated/schema.d.ts";
import type { ObopMeta } from "./meta.ts";

/** What `getToken` may answer: the bearer token, or nothing when nobody is signed in. */
export type Token = string | null | undefined;

/**
 * What `getLanguage` may answer: a language tag (`sv`, `de-AT`) or a full `Accept-Language` value
 * (`sv-SE,sv;q=0.9,en;q=0.5`), or nothing to send none.
 */
export type Language = string | null | undefined;

export interface ObopClientOptions {
  /** The address of the instance, for example `https://acme.example.com`. A trailing slash is ignored. */
  baseUrl: string;
  /**
   * The bearer token to send, asked for on every call (so a refreshed token is used at once). May
   * answer later (a secure store is asynchronous). With no token the call is sent without
   * `Authorization`. If it throws, the call rejects with that error and nothing is sent.
   */
  getToken: () => Token | Promise<Token>;
  /**
   * The language the app shows, asked for on every call and sent as `Accept-Language`, so that the
   * instance words its refusals in it where a package translated them (the message of an assertion,
   * which comes with its `assert_key`): the language of the app, which may not be the one of the
   * device. Only the seven languages obop ships count (`de en es fi fr pl sv`); a region reads as
   * its language, and an unsupported one is skipped by the server. With no language, or if this
   * throws or answers something that is no header value, the call is sent without the header and
   * the texts are the package's own. A header the call carries already is not replaced.
   */
  getLanguage?: () => Language | Promise<Language>;
  /** The `fetch` to use; the global one when absent (it is looked up at each call, so a test may replace it). */
  fetch?: typeof fetch;
  /**
   * Called when the instance answers 401 to a call that carried a token: the token is missing,
   * expired or revoked. The client does not sign in again or retry; this is where the app signs the
   * person out or starts a refresh. Calls that fail at the same moment share one run of it, and an
   * error it throws is ignored (the call still rejects with the 401).
   *
   * The handler may use this client, for example to refresh the token: a call made while the
   * handler runs neither waits for it nor starts it again. If that call is answered 401 too, it
   * rejects with the `ObopApiError` (the handler sees it), and the handler does not wait for
   * itself. A call that was made before the handler began and is answered 401 while it runs waits
   * for it, so a handler must not wait for such a call.
   */
  onUnauthorized?: () => void | Promise<void>;
}

/** The two options every call takes. */
export interface CallOptions {
  signal?: AbortSignal;
}

/** The answer of a command: `value`, and the names of the fields withheld from this caller. */
export interface CallResult<T> {
  value: T;
  denied: string[];
}

/** The models that have `bulk_create` in the generated schema (every model that can create). */
export type BulkModel = BulkModelOf<keyof paths>;
type BulkModelOf<P> = P extends `/api/${infer M}/bulk_create` ? M : never;
type BulkOperation<M extends BulkModel> = paths[`/api/${M}/bulk_create`]["post"];

/** One row of `bulkCreate` for a model of the generated schema: its writable fields. */
export type BulkRow<M extends BulkModel> = NonNullable<
  BulkOperation<M>["requestBody"]["content"]["application/json"]["rows"]
>[number];

/**
 * The report of a `bulk_create` call (D70): `ok`, `total`, `created`, `failed`, `errors`, `ids` (in
 * the order of the rows), `dry_run`, `valid` and `replayed`, which is `true` when the same key and
 * rows had been sent before and this is the first answer again.
 */
export type BulkReport<M extends BulkModel = BulkModel> = NonNullable<
  BulkOperation<M>["responses"][200]["content"]["application/json"]["value"]
>;

export interface BulkCreateOptions extends CallOptions {
  /**
   * 1 to 128 visible ASCII characters. The same key with the same rows and mode returns the first
   * answer (`replayed: true`) and creates nothing, for 24 hours; the same key with other rows is
   * refused. Sent as `idempotency_key`; a dry run ignores it. Without a key a repeat after a lost
   * answer creates the rows twice.
   */
  idempotencyKey?: string;
  /** `all_or_nothing` (the server's default) or `partial`, which keeps the good rows. */
  mode?: "all_or_nothing" | "partial";
  /** Only check the rows. */
  dryRun?: boolean;
}

export interface ObopClient {
  /**
   * The `openapi-fetch` client over the generated `paths`, with the token, the 401 handler and the
   * error mapping installed: `client.api.POST("/api/party/list")`. A call that does not succeed,
   * or succeeds with a body that is not JSON (`invalid_response`), rejects with `ObopApiError`;
   * `error` is never set on what it resolves with. Calls with another `parseAs` than `json`
   * (a download) are not read by the client and are returned as they come.
   */
  readonly api: Client<paths>;
  /**
   * Run any command, `POST /api/<model>/<command>` with a JSON body, also for a model that the
   * generated schema does not know (a package installed on the instance later). Resolves with the
   * `value` of the answer. Rejects with `ObopApprovalRequired` when an agent token only proposed it.
   */
  call<T = unknown>(
    model: string,
    command: string,
    input?: object,
    opts?: CallOptions,
  ): Promise<CallResult<T>>;
  /**
   * `<model>.bulk_create` (D70): up to 1,000 rows in one call. For a model of the generated schema
   * the rows are typed from its writable fields.
   */
  bulkCreate<M extends BulkModel>(
    model: M,
    rows: readonly BulkRow<M>[],
    opts?: BulkCreateOptions,
  ): Promise<BulkReport<M>>;
  /**
   * A model that the generated schema does not know, for example one of a package installed on the
   * instance later: untyped rows. A model name the schema does know must use the typed form above,
   * so this form is closed to the literal names of `BulkModel` (a plain `string` still fits).
   */
  bulkCreate<M extends string>(
    model: string extends M ? M : M extends BulkModel ? never : M,
    rows: readonly Record<string, unknown>[],
    opts?: BulkCreateOptions,
  ): Promise<BulkReport>;
  /** `GET /api/meta`: the modules, models, fields and actions this token may use. */
  getMeta(opts?: CallOptions): Promise<ObopMeta>;
}

/** An `idempotency_key` the server accepts (D70). */
const IDEMPOTENCY_KEY = /^[\x21-\x7e]{1,128}$/;

type Untyped = {
  POST(
    path: string,
    init: { body?: unknown; signal?: AbortSignal },
  ): Promise<{ data?: unknown; response: Response }>;
};

export function createObopClient(options: ObopClientOptions): ObopClient {
  const baseUrl = options.baseUrl.replace(/\/+$/, "");
  const api = createClient<paths>({
    baseUrl,
    fetch: (request) => (options.fetch ?? globalThis.fetch)(request),
  });

  // 401s that arrive together share one run of onUnauthorized. While it runs, calls that are made
  // (the handler's own refresh through this client, or any other) neither wait for it nor start
  // it again: their 401 rejects the call like any other, or the handler would wait for itself.
  let unauthorized: Promise<void> | undefined;
  let handlerRunning = false;
  const issuedDuringHandler = new Set<string>();
  const signedOut = (): Promise<void> => {
    unauthorized ??= (async () => {
      handlerRunning = true;
      try {
        await options.onUnauthorized?.();
      } catch {
        // The call still rejects with the 401; a failing handler must not hide it.
      } finally {
        handlerRunning = false;
      }
    })().finally(() => {
      unauthorized = undefined;
    });
    return unauthorized;
  };

  const callInfo = (request: Request, schemaPath: string): CallInfo => ({
    method: request.method,
    path: schemaPath,
    read: isReadCall(request.method, schemaPath),
  });

  // The language is a preference, not a credential: a hook that fails, or a value that is no
  // header, must not take the call down with it.
  const setLanguage = async (request: Request): Promise<void> => {
    if (!options.getLanguage || request.headers.has("accept-language")) return;
    try {
      const language = await options.getLanguage();
      if (typeof language === "string" && language.trim() !== "") {
        request.headers.set("accept-language", language.trim());
      }
    } catch {
      // Sent without it.
    }
  };

  const middleware: Middleware = {
    async onRequest({ request, id }) {
      const duringHandler = handlerRunning;
      const token = await options.getToken();
      if (duringHandler) issuedDuringHandler.add(id);
      if (token) request.headers.set("authorization", `Bearer ${token}`);
      await setLanguage(request);
      return request;
    },
    async onResponse({ request, response, schemaPath, options: sent, id }) {
      const info = callInfo(request, schemaPath);
      const duringHandler = issuedDuringHandler.delete(id);
      if (response.ok) {
        // A success the program cannot read must not escape as a SyntaxError of the parser that
        // openapi-fetch runs next: the address may not be an obop instance at all.
        if (sent.parseAs !== "json") return undefined;
        let text: string;
        try {
          text = await response.text();
        } catch (cause) {
          throw new ObopNetworkError(cause, info);
        }
        if (text.trim() !== "") {
          try {
            JSON.parse(text);
          } catch {
            throw invalidResponse(response, text, info);
          }
        }
        // The body is read, so openapi-fetch gets the same answer again (a null body for 204).
        return new Response(text === "" ? null : text, {
          status: response.status,
          statusText: response.statusText,
          headers: response.headers,
        });
      }
      const error = await ObopApiError.fromResponse(response, info);
      if (response.status === 401 && request.headers.has("authorization") && !duringHandler) {
        await signedOut();
      }
      throw error;
    },
    onError({ request, error, schemaPath, id }) {
      issuedDuringHandler.delete(id);
      // The caller cancelled the call: that is theirs to handle, not a failure of the network.
      if (error instanceof Error && error.name === "AbortError") return undefined;
      return new ObopNetworkError(error, callInfo(request, schemaPath));
    },
  };
  api.use(middleware);

  const untyped = api as unknown as Untyped;

  async function call<T = unknown>(
    model: string,
    command: string,
    input: object = {},
    opts: CallOptions = {},
  ): Promise<CallResult<T>> {
    const path = `/api/${encodeURIComponent(model)}/${encodeURIComponent(command)}`;
    const info: CallInfo = { method: "POST", path, read: isReadCall("POST", path) };
    const { data, response } = await untyped.POST(path, { body: input, signal: opts.signal });
    // Every command answers an object; JSON of another shape is some other server's answer.
    const answer: unknown = data ?? {};
    if (typeof answer !== "object" || answer === null || Array.isArray(answer)) {
      throw invalidResponse(response, JSON.stringify(data), info);
    }
    const body = answer as { value?: T; denied?: string[]; plan_id?: string };
    if (response.status === 202) throw new ObopApprovalRequired(String(body.plan_id ?? ""), info);
    return { value: body.value as T, denied: body.denied ?? [] };
  }

  async function bulkCreate(
    model: string,
    rows: readonly Record<string, unknown>[],
    opts: BulkCreateOptions = {},
  ): Promise<BulkReport> {
    if (opts.idempotencyKey !== undefined && !IDEMPOTENCY_KEY.test(opts.idempotencyKey)) {
      throw new RangeError("idempotencyKey must be 1 to 128 visible ASCII characters");
    }
    const input: Record<string, unknown> = { rows };
    if (opts.mode !== undefined) input.mode = opts.mode;
    if (opts.dryRun !== undefined) input.dry_run = opts.dryRun;
    if (opts.idempotencyKey !== undefined) input.idempotency_key = opts.idempotencyKey;
    const { value } = await call<BulkReport>(model, "bulk_create", input, opts);
    return value;
  }

  async function getMeta(opts: CallOptions = {}): Promise<ObopMeta> {
    const { data, response } = await api.GET("/api/meta", { signal: opts.signal });
    // The first thing an app asks of an address the person typed: JSON that is not the metadata of
    // an instance (no modules) means the address is some other server.
    const meta = data as unknown as Partial<ObopMeta> | null | undefined;
    if (meta === null || typeof meta !== "object" || !Array.isArray(meta.modules)) {
      throw invalidResponse(response, JSON.stringify(data) ?? "", {
        method: "GET",
        path: "/api/meta",
        read: true,
      });
    }
    return meta as ObopMeta;
  }

  return { api, call, bulkCreate: bulkCreate as ObopClient["bulkCreate"], getMeta };
}

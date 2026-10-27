import createClient, { type Client, type Middleware } from "openapi-fetch";
import {
  ObopApiError,
  ObopApprovalRequired,
  ObopNetworkError,
  isReadCall,
  type CallInfo,
} from "./errors.ts";
import type { paths } from "./generated/schema.d.ts";
import type { ObopMeta } from "./meta.ts";

/** What `getToken` may answer: the bearer token, or nothing when nobody is signed in. */
export type Token = string | null | undefined;

export interface ObopClientOptions {
  /** The address of the instance, for example `https://acme.example.com`. A trailing slash is ignored. */
  baseUrl: string;
  /**
   * The bearer token to send, asked for on every call (so a refreshed token is used at once). May
   * answer later (a secure store is asynchronous). With no token the call is sent without
   * `Authorization`. If it throws, the call rejects with that error and nothing is sent.
   */
  getToken: () => Token | Promise<Token>;
  /** The `fetch` to use; the global one when absent (it is looked up at each call, so a test may replace it). */
  fetch?: typeof fetch;
  /**
   * Called when the instance answers 401 to a call that carried a token: the token is missing,
   * expired or revoked. The client does not sign in again or retry; this is where the app signs the
   * person out or starts a refresh. Calls that fail at the same moment share one run of it, and an
   * error it throws is ignored (the call still rejects with the 401).
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
   * error mapping installed: `client.api.POST("/api/party/list", { body: {} })`. A call that does
   * not succeed rejects with `ObopApiError`; `error` is never set on what it resolves with.
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

  // 401s that arrive together share one run of onUnauthorized.
  let unauthorized: Promise<void> | undefined;
  const signedOut = (): Promise<void> => {
    unauthorized ??= (async () => {
      try {
        await options.onUnauthorized?.();
      } catch {
        // The call still rejects with the 401; a failing handler must not hide it.
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

  const middleware: Middleware = {
    async onRequest({ request }) {
      const token = await options.getToken();
      if (token) request.headers.set("authorization", `Bearer ${token}`);
      return request;
    },
    async onResponse({ request, response, schemaPath }) {
      if (response.ok) return undefined;
      const error = await ObopApiError.fromResponse(response, callInfo(request, schemaPath));
      if (response.status === 401 && request.headers.has("authorization")) await signedOut();
      throw error;
    },
    onError({ request, error, schemaPath }) {
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
    const { data, response } = await untyped.POST(path, { body: input, signal: opts.signal });
    const body = (data ?? {}) as { value?: T; denied?: string[]; plan_id?: string };
    if (response.status === 202) {
      throw new ObopApprovalRequired(String(body.plan_id ?? ""), {
        method: "POST",
        path,
        read: false,
      });
    }
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
    const { data } = await api.GET("/api/meta", { signal: opts.signal });
    return data as unknown as ObopMeta;
  }

  return { api, call, bulkCreate: bulkCreate as ObopClient["bulkCreate"], getMeta };
}

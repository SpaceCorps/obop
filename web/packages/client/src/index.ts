// @obop/client: the typed client of an obop instance. Platform neutral: it needs the global
// `fetch`, `Request`, `Response` and `Headers` and nothing else (no DOM, no Node-only API), so the
// web back office and the React Native apps import the same code.
export { createObopClient } from "./client.ts";
export type {
  BulkCreateOptions,
  BulkModel,
  BulkReport,
  BulkRow,
  CallOptions,
  CallResult,
  ObopClient,
  ObopClientOptions,
  Token,
} from "./client.ts";
export {
  ObopApiError,
  ObopApprovalRequired,
  ObopNetworkError,
  isReadCall,
  isSafeToRetry,
  parseApplied,
  parseRetryAfter,
  retryDelayMs,
} from "./errors.ts";
export type { Applied, CallInfo, ObopApiErrorInit } from "./errors.ts";
export type {
  ObopActionMeta,
  ObopFieldMeta,
  ObopMeta,
  ObopModelMeta,
  ObopModuleMeta,
} from "./meta.ts";
export type { components, operations, paths } from "./generated/schema.d.ts";

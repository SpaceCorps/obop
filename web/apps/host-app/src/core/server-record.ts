// The one thing the app keeps on the phone: which server it shows. The item is a JSON object,
// `{ "v": 1, "origin": "...", "path": "/", "coreVersion": "0.2.1", "addedAt": 1763200000000 }`.
// The address is not a secret; it is read back without trusting anything about its shape.
import { originOf } from "@obop/host-bridge";
import type { Address } from "./address.ts";

export interface ServerRecord extends Address {
  readonly v: 1;
  /** `info.version` of the API description at the time the address was checked. */
  readonly coreVersion: string;
  /** When the address was stored, in milliseconds since 1970. */
  readonly addedAt: number;
}

const PATH = /^\/(?:[a-z0-9_-]{1,32}\/)?$/;
const MAX_ITEM_CHARS = 2_048;
/** How long the host bar says "new since". */
export const NEW_FOR_MS = 7 * 24 * 60 * 60 * 1000;

export function makeServerRecord(address: Address, coreVersion: string, now: number): ServerRecord {
  return { v: 1, origin: address.origin, path: address.path, coreVersion, addedAt: now };
}

export function encodeServerRecord(record: ServerRecord): string {
  return JSON.stringify({
    v: record.v,
    origin: record.origin,
    path: record.path,
    coreVersion: record.coreVersion,
    addedAt: record.addedAt,
  });
}

/**
 * The stored item as a record, or `null` when it is missing or is not one: not JSON, another
 * version, an origin that is not written as `originOf` writes one, a path that is not `/` or one
 * segment between slashes. An app that reads `null` asks for the address again.
 */
export function decodeServerRecord(text: string | null | undefined): ServerRecord | null {
  if (typeof text !== "string" || text.length > MAX_ITEM_CHARS) return null;
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const item = value as Record<string, unknown>;
  if (item.v !== 1) return null;
  const { origin, path, coreVersion, addedAt } = item;
  if (typeof origin !== "string" || originOf(origin) !== origin) return null;
  if (typeof path !== "string" || !PATH.test(path)) return null;
  if (typeof coreVersion !== "string" || coreVersion.length > 64) return null;
  if (typeof addedAt !== "number" || !Number.isFinite(addedAt) || addedAt < 0) return null;
  return { v: 1, origin, path, coreVersion, addedAt };
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** A day as the native screens write it, in the phone's own time zone: `15 Nov 2026`. */
export function dayText(time: number): string {
  const date = new Date(time);
  return `${date.getDate()} ${MONTHS[date.getMonth()] ?? ""} ${date.getFullYear()}`;
}

/**
 * The day the address was stored, while that is less than 7 days ago, else `null`. The host bar
 * says "new since" with it: a notice that is shown once is seen and dismissed by whoever opens the
 * app next, which can be the person who changed the server.
 */
export function newSince(record: ServerRecord, now: number): string | null {
  const age = now - record.addedAt;
  return age >= 0 && age < NEW_FOR_MS ? dayText(record.addedAt) : null;
}

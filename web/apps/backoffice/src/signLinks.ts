// The logic behind the "Create signing link" button of a sign request (D92, docs/SIGN.md). Pure
// functions, so a test can pin them without a DOM: when the button is available, what the server's
// answer to `sign_link.create` must hold, and how the link is shown.
//
// The link is a secret: it is in the answer to `create` once, the server keeps only a hash of it,
// and nothing here stores it, logs it or puts it in a toast. It lives in the state of one dialog.

import { t } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { optionLabel } from "./format.ts";
import { fieldTitle } from "./labels.ts";

/** The model of the links and the command that makes one. */
export const LINK_MODEL = "sign_link";
export const LINK_CREATE = "sign_link.create";
export const LINK_UPDATE = "sign_link.update";

/** The request is the record of this model. */
export const REQUEST_MODEL = "sign_request";

/** Whether a link may be made for this request now, and if not, why (a sentence). */
export type LinkGate = { ok: true } | { ok: false; reason: string };

/**
 * A link is made for a request that has been sent and that points at a document. The server checks
 * both again (and that the document did not change since the request was sent); this only tells the
 * person early, instead of letting them press a button that will be refused.
 */
export function linkGate(row: Row, fields: FieldMeta[]): LinkGate {
  const state = fields.find((f) => f.name === "state");
  if (state && String(row.state ?? "") !== "sent")
    return {
      ok: false,
      reason: t("Only available when {field} is {state}", {
        field: fieldTitle(state),
        state: optionLabel("sent", state),
      }),
    };
  if (!String(row.document_id ?? "").trim())
    return { ok: false, reason: t("The request has no document to sign") };
  return { ok: true };
}

/** The state of a link as the list shows it (the model has no state field, D92). */
export type LinkStatus = "active" | "used" | "revoked" | "expired" | "closed";

const set = (v: unknown): boolean => typeof v === "string" && v.trim() !== "";

/** The end of the day `YYYY-MM-DD` in UTC, as the door takes it (23:59:59.999999); NaN if unreadable. */
function endOfDay(date: unknown): number {
  const m = typeof date === "string" ? /^(\d{4}-\d{2}-\d{2})/.exec(date.trim()) : null;
  return m ? Date.parse(`${m[1]}T23:59:59.999Z`) : Number.NaN;
}

/**
 * The status of a link, worked out the way the signer's door works it out (`status` in
 * `obop-commands/src/sign/door.rs`), in the same order:
 *
 * 1. `used_at` set: used. 2. `revoked_at` set: revoked.
 * 3. The request is signed (by another link): closed.
 * 4. Past the link's own end, or past the end of the request's `expires_on` day, whichever comes
 *    first: expired. A link with no readable end does not work, so it reads as expired.
 * 5. The request is anything but `sent` (declined, expired, still a draft): closed.
 * 6. Otherwise active.
 *
 * `request` is the sign request the link belongs to; without it the checks that need it are
 * skipped. The server makes the same decision at the door on its own clock. This list uses the
 * browser's clock for the two ends, so it can be a little off near an end and wrong by as much as
 * the browser's clock is wrong: it is a picture of the door, and the door decides. That is why
 * Revoke is offered for every link that is neither used nor revoked (`canRevoke`), not only for
 * one that reads "active".
 */
export function linkStatus(row: Row, nowMs: number, request?: Row): LinkStatus {
  if (set(row.used_at)) return "used";
  if (set(row.revoked_at)) return "revoked";
  const state = request ? String(request.state ?? "") : "";
  if (request && state === "signed") return "closed";
  let end = set(row.expires_at) ? Date.parse(String(row.expires_at)) : Number.NaN;
  if (!Number.isFinite(end)) end = Number.NEGATIVE_INFINITY;
  const day = request ? endOfDay(request.expires_on) : Number.NaN;
  if (Number.isFinite(day)) end = Math.min(end, day);
  if (nowMs > end) return "expired";
  if (request && state !== "sent") return "closed";
  return "active";
}

/**
 * What a try to close the dialog that shows the link does. Escape, a click outside, the X and Done
 * all ask this. Once the link was copied (by the button or from the field) it closes. Before that
 * the first try only asks "close without copying?", and a second try goes back to the link: only
 * the button of the question closes it, so a double keypress or a stray click cannot lose it.
 */
export function closeAttempt(
  kept: boolean,
  leaving: boolean,
): { close: boolean; leaving: boolean } {
  if (kept) return { close: true, leaving: false };
  return { close: false, leaving: !leaving };
}

/** A link that was not used and not revoked may be revoked, whatever this page thinks of its end. */
export const canRevokeStatus = (s: LinkStatus): boolean => s !== "used" && s !== "revoked";

/** The body of the update that revokes a link: the server stamps its own clock and refuses the rest. */
export function revokeBody(id: string, now: Date): { id: string; revoked_at: string } {
  return { id, revoked_at: now.toISOString() };
}

/** What the answer of `sign_link.create` gives the dialog. */
export type MadeLink = { href: string; expiresAt: string };

/**
 * The link as the person should copy it, from the server's answer. The server returns
 * `<OBOP_PUBLIC_URL>/sign/...` when that setting exists and the path `/sign/...` when it does not;
 * a path gets this page's origin (the back office and the signer's page are on the same origin,
 * the page itself is not under the admin prefix). Anything that is not an http or https address
 * of a `/sign/` path is refused: the dialog never shows a thing it did not understand.
 */
export function madeLink(value: unknown, origin: string): MadeLink | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  if (typeof v.link !== "string" || !v.link) return null;
  let url: URL;
  try {
    url = new URL(v.link, origin);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  if (!url.pathname.startsWith("/sign/") || url.search || url.hash || url.username) return null;
  return { href: url.href, expiresAt: typeof v.expires_at === "string" ? v.expires_at : "" };
}

/**
 * Whether a copy event in the link's field copied the whole link. A copy with part of it selected, or
 * with only a caret in the field, puts a fragment or nothing on the clipboard and must not count as
 * the link being kept.
 */
export function copiedWholeLink(input: HTMLInputElement, href: string): boolean {
  return (
    input.value === href && input.selectionStart === 0 && input.selectionEnd === input.value.length
  );
}

/** The text of a link that cannot be copied by the clipboard API: select it for the person. */
export function selectAll(input: HTMLInputElement | null): void {
  if (!input) return;
  input.focus();
  input.select();
}

/** Put `text` on the clipboard. `false` when the browser refuses (then the field is selected). */
export async function copyText(text: string, input: HTMLInputElement | null): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    selectAll(input);
    try {
      return document.execCommand("copy");
    } catch {
      return false;
    }
  }
}

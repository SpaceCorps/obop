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

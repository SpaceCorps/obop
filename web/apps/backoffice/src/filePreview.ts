/**
 * The preview of a PDF or a picture that a record holds in a file field (Documents, D60).
 *
 * The bytes come from the same bearer-token fetch as a download (`GET /api/blobs/{id}`); there is no
 * new server route. The server answers a file with `Content-Disposition: attachment` and a CSP with
 * `sandbox`, which is right for a download and keeps a PDF out of a frame, so the page makes its own
 * `Blob` from the bytes and gives it the one type it has decided to show:
 *
 * - only `application/pdf`, `image/png` and `image/jpeg` are previewed; every other type, `text/html`
 *   and SVG included, stays a download and never becomes an object URL of the page;
 * - the type of the Blob is the page's own choice, never the server's header copied as it came, and
 *   the first bytes must agree with it (a file that says PDF but is not one is not shown);
 * - a file over 8 MiB is not fetched until the person asks, so opening a record never pulls a large
 *   file on its own.
 *
 * Everything here is pure or takes its collaborators as arguments, so a test can pin it without a
 * browser. The component is `FilePreview` in `BlobFields.tsx`.
 */

/** The size above which the preview waits for a click (the server takes files up to 25 MiB). */
export const PREVIEW_LIMIT = 8 * 1024 * 1024;

export type PreviewKind = "pdf" | "image";

/** What may be previewed: the media type the page gives the Blob, and how it is shown. */
const SHOWN: Record<string, PreviewKind> = {
  "application/pdf": "pdf",
  "image/png": "image",
  "image/jpeg": "image",
};

/**
 * The media type of a `Content-Type` header without its parameters, in lower case, when it is one of
 * the types the page previews; `null` for everything else (and for a missing or odd header).
 */
export function previewType(contentType: string | null | undefined): string | null {
  const type = (contentType ?? "").split(";")[0]!.trim().toLowerCase();
  return Object.hasOwn(SHOWN, type) ? type : null;
}

export function previewKind(type: string): PreviewKind | null {
  return Object.hasOwn(SHOWN, type) ? SHOWN[type]! : null;
}

const startsWith = (b: Uint8Array, at: number, sig: number[]) =>
  b.length >= at + sig.length && sig.every((v, i) => b[at + i] === v);

/**
 * Whether the first bytes are those of `type`. A PDF may have a few bytes before `%PDF-` (the format
 * allows up to 1024), a PNG and a JPEG start with their signature.
 */
export function looksLike(type: string, bytes: Uint8Array): boolean {
  switch (type) {
    case "application/pdf": {
      const head = bytes.subarray(0, 1024 + 5);
      for (let i = 0; i + 5 <= head.length; i++)
        if (startsWith(head, i, [37, 80, 68, 70, 45])) return true;
      return false;
    }
    case "image/png":
      return startsWith(bytes, 0, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    case "image/jpeg":
      return startsWith(bytes, 0, [0xff, 0xd8, 0xff]);
    default:
      return false;
  }
}

export type Preview = { kind: PreviewKind; type: string; url: string; size: number };

export type PreviewResult =
  | { ok: Preview }
  /** Too large to fetch unasked: the size in bytes. */
  | { large: number }
  /** Not a type that is previewed, or bytes that do not match their type. */
  | { none: true }
  | { failed: true };

type Env = {
  fetchFn?: typeof fetch;
  makeUrl?: (blob: Blob) => string;
};

/**
 * Fetch the file and make it ready to show. `force` fetches a file over the limit as well; without
 * it the body of such a file is dropped unread as soon as the headers have said how big it is.
 * Nothing is created when `signal` has been aborted by the time the bytes arrive, so a person who
 * moves to another record leaves no object URL behind.
 */
export async function loadPreview(
  token: string,
  id: string,
  opts: { force?: boolean; signal?: AbortSignal } & Env = {},
): Promise<PreviewResult> {
  const fetchFn = opts.fetchFn ?? fetch;
  const makeUrl = opts.makeUrl ?? ((b: Blob) => URL.createObjectURL(b));
  try {
    const r = await fetchFn(`/api/blobs/${encodeURIComponent(id)}`, {
      headers: { authorization: `Bearer ${token}` },
      signal: opts.signal,
    });
    if (!r.ok) return { failed: true };
    const type = previewType(r.headers.get("content-type"));
    if (!type) {
      void r.body?.cancel().catch(() => {});
      return { none: true };
    }
    const declared = Number(r.headers.get("content-length"));
    if (!opts.force && Number.isFinite(declared) && declared > PREVIEW_LIMIT) {
      void r.body?.cancel().catch(() => {});
      return { large: declared };
    }
    const bytes = new Uint8Array(await r.arrayBuffer());
    if (!opts.force && bytes.length > PREVIEW_LIMIT) return { large: bytes.length };
    if (opts.signal?.aborted) return { failed: true };
    if (!looksLike(type, bytes)) return { none: true };
    const kind = previewKind(type)!;
    return { ok: { kind, type, size: bytes.length, url: makeUrl(new Blob([bytes], { type })) } };
  } catch {
    return { failed: true };
  }
}

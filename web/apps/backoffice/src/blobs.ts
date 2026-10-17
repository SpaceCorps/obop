/** Images and files held by records (D60): upload with progress, fetch with the session token. */

export const IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"];
export const IMAGE_ACCEPT = IMAGE_TYPES.join(",");
export const FILE_ACCEPT = [
  "application/pdf",
  "text/plain",
  "text/csv",
  "application/zip",
  ".docx",
  ".xlsx",
  ".pptx",
  ".odt",
  ".ods",
  ".odp",
  ...IMAGE_TYPES,
].join(",");

/** The sizes the server makes thumbnails at; the closest one that is large enough is asked for. */
export const THUMB_WIDTHS = [320, 640, 960, 1280] as const;

export function thumbWidth(cssPixels: number): number {
  const need = cssPixels * (typeof devicePixelRatio === "number" ? devicePixelRatio : 1);
  return THUMB_WIDTHS.find((w) => w >= need) ?? THUMB_WIDTHS[THUMB_WIDTHS.length - 1]!;
}

export function formatBytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(n >= 10 * 1024 * 1024 ? 0 : 1)} MiB`;
  if (n >= 1024) return `${Math.round(n / 1024)} KiB`;
  return `${n} B`;
}

export type Uploaded = {
  id: string;
  content_type: string;
  size: number;
  width: number | null;
  height: number | null;
};

export type Upload = {
  promise: Promise<{ ok: Uploaded } | { error: string; status: number }>;
  abort: () => void;
};

/** Send `file` to `POST /api/blobs?field=model.field`, reporting progress from 0 to 1. */
export function uploadBlob(
  token: string,
  target: string,
  file: File,
  onProgress: (fraction: number) => void,
): Upload {
  const xhr = new XMLHttpRequest();
  const promise = new Promise<{ ok: Uploaded } | { error: string; status: number }>((resolve) => {
    xhr.open(
      "POST",
      `/api/blobs?field=${encodeURIComponent(target)}&filename=${encodeURIComponent(file.name)}`,
    );
    xhr.setRequestHeader("authorization", `Bearer ${token}`);
    xhr.setRequestHeader("content-type", file.type || "application/octet-stream");
    xhr.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total);
    xhr.onerror = () => resolve({ error: "", status: 0 });
    xhr.onabort = () => resolve({ error: "", status: -1 });
    xhr.onload = () => {
      let body: Record<string, unknown> = {};
      try {
        body = JSON.parse(xhr.responseText) as Record<string, unknown>;
      } catch {
        /* not JSON */
      }
      if (xhr.status === 201 && typeof body.id === "string") resolve({ ok: body as Uploaded });
      else resolve({ error: String(body.error ?? ""), status: xhr.status });
    };
    xhr.send(file);
  });
  return { promise, abort: () => xhr.abort() };
}

const urls = new Map<string, Promise<string | null>>();

/** An object URL for the blob (or a thumbnail of it), fetched once per session and size. */
export function blobUrl(token: string, id: string, width?: number): Promise<string | null> {
  const key = `${token.slice(-8)}:${id}:${width ?? 0}`;
  let p = urls.get(key);
  if (!p) {
    p = fetch(`/api/blobs/${id}${width ? `?w=${width}` : ""}`, {
      headers: { authorization: `Bearer ${token}` },
    })
      .then(async (r) => (r.ok ? URL.createObjectURL(await r.blob()) : null))
      .catch(() => null);
    urls.set(key, p);
    // A failed fetch is not remembered, so the next render may try again.
    void p.then((u) => u === null && urls.delete(key));
  }
  return p;
}

/** Download a file: the bytes need the session token, so a plain link would not work. */
export async function saveBlob(token: string, id: string, fallbackName: string): Promise<boolean> {
  try {
    const r = await fetch(`/api/blobs/${id}`, { headers: { authorization: `Bearer ${token}` } });
    if (!r.ok) return false;
    const cd = r.headers.get("content-disposition") ?? "";
    const star = /filename\*=UTF-8''([^;]+)/i.exec(cd)?.[1];
    let name = fallbackName;
    try {
      if (star) name = decodeURIComponent(star);
    } catch {
      /* keep the fallback */
    }
    const url = URL.createObjectURL(await r.blob());
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
    return true;
  } catch {
    return false;
  }
}

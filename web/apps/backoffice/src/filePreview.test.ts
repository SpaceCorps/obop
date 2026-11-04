import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vite-plus/test";
import { PREVIEW_LIMIT, loadPreview, looksLike, previewKind, previewType } from "./filePreview.ts";

// A one-page PDF made up for this test (its text says so); no real document is used anywhere.
const PDF = new Uint8Array(
  readFileSync(new URL("./fixtures/synthetic-agreement.pdf", import.meta.url)),
);
const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const JPEG = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 0x10, 0x4a, 0x46]);
const HTML = new TextEncoder().encode("<html><script>alert(1)</script></html>");

describe("the types that are previewed", () => {
  it("are PDF, PNG and JPEG, whatever the case or the parameters of the header", () => {
    expect(previewType("application/pdf")).toBe("application/pdf");
    expect(previewType("Application/PDF; charset=binary")).toBe("application/pdf");
    expect(previewType(" image/png ")).toBe("image/png");
    expect(previewType("image/jpeg")).toBe("image/jpeg");
    expect(previewKind("application/pdf")).toBe("pdf");
    expect(previewKind("image/png")).toBe("image");
  });

  it("are nothing else: HTML, SVG, scripts, archives, office files and odd headers stay downloads", () => {
    for (const type of [
      "text/html",
      "image/svg+xml",
      "application/xhtml+xml",
      "text/javascript",
      "application/zip",
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      "text/plain",
      "image/gif",
      "image/webp",
      "application/pdf-evil",
      "application/octet-stream",
      "constructor",
      "__proto__",
      "toString",
      "",
      null,
      undefined,
    ])
      expect(previewType(type), String(type)).toBeNull();
    expect(previewKind("text/html")).toBeNull();
    expect(previewKind("__proto__")).toBeNull();
  });
});

describe("the first bytes of a file", () => {
  it("are checked against the type it claims", () => {
    expect(looksLike("application/pdf", PDF)).toBe(true);
    expect(looksLike("image/png", PNG)).toBe(true);
    expect(looksLike("image/jpeg", JPEG)).toBe(true);
    expect(looksLike("application/pdf", HTML)).toBe(false);
    expect(looksLike("application/pdf", PNG)).toBe(false);
    expect(looksLike("image/png", PDF)).toBe(false);
    expect(looksLike("image/jpeg", PNG)).toBe(false);
    expect(looksLike("text/html", HTML)).toBe(false);
    expect(looksLike("application/pdf", new Uint8Array())).toBe(false);
  });

  it("allow the junk before %PDF- that the format allows, up to 1024 bytes", () => {
    const pad = (n: number) => Uint8Array.from([...new Uint8Array(n).fill(32), ...PDF]);
    expect(looksLike("application/pdf", pad(100))).toBe(true);
    expect(looksLike("application/pdf", pad(1024))).toBe(true);
    expect(looksLike("application/pdf", pad(1030))).toBe(false);
  });
});

/** A fetch that answers with `bytes` and the given headers, and records what it was asked. */
function server(bytes: Uint8Array, headers: Record<string, string>, status = 200) {
  const calls: { url: string; init: RequestInit | undefined }[] = [];
  const fn = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return new Response(bytes as BodyInit, { status, headers });
  });
  return { fn: fn as unknown as typeof fetch, calls };
}

describe("fetching the preview of a stored file", () => {
  const made: Blob[] = [];
  const makeUrl = (b: Blob) => {
    made.push(b);
    return `blob:test/${made.length}`;
  };
  const reset = () => (made.length = 0);

  it("asks for the bytes with the bearer token, as a download does, and builds a typed Blob of them", async () => {
    reset();
    const s = server(PDF, {
      "content-type": "application/pdf",
      "content-length": String(PDF.length),
    });
    const r = await loadPreview("tok-1", "blob-id", { fetchFn: s.fn, makeUrl });
    expect(s.calls).toHaveLength(1);
    expect(s.calls[0]!.url).toBe("/api/blobs/blob-id");
    expect(s.calls[0]!.init?.headers).toEqual({ authorization: "Bearer tok-1" });
    expect(r).toEqual({
      ok: { kind: "pdf", type: "application/pdf", url: "blob:test/1", size: PDF.length },
    });
    expect(made).toHaveLength(1);
    expect(made[0]!.type).toBe("application/pdf");
    expect(new Uint8Array(await made[0]!.arrayBuffer())).toEqual(PDF);
  });

  it("gives the Blob the page's own type, not the server's header as it came", async () => {
    reset();
    const s = server(PDF, { "content-type": "Application/PDF; charset=binary" });
    const r = await loadPreview("t", "id", { fetchFn: s.fn, makeUrl });
    expect("ok" in r).toBe(true);
    expect(made[0]!.type).toBe("application/pdf");
  });

  it("shows pictures too", async () => {
    reset();
    const r = await loadPreview("t", "id", {
      fetchFn: server(PNG, { "content-type": "image/png" }).fn,
      makeUrl,
    });
    expect(r).toMatchObject({ ok: { kind: "image", type: "image/png" } });
    const j = await loadPreview("t", "id", {
      fetchFn: server(JPEG, { "content-type": "image/jpeg" }).fn,
      makeUrl,
    });
    expect(j).toMatchObject({ ok: { kind: "image", type: "image/jpeg" } });
  });

  it("makes no object URL for a type that is not previewed, even when the bytes are a PDF", async () => {
    reset();
    for (const type of [
      "text/html",
      "image/svg+xml",
      "application/zip",
      "application/octet-stream",
      "",
    ]) {
      const headers: Record<string, string> = type ? { "content-type": type } : {};
      const r = await loadPreview("t", "id", { fetchFn: server(PDF, headers).fn, makeUrl });
      expect(r, type).toEqual({ none: true });
    }
    expect(made).toHaveLength(0);
  });

  it("makes no object URL for bytes that do not match the type they claim", async () => {
    reset();
    for (const type of ["application/pdf", "image/png", "image/jpeg"]) {
      const r = await loadPreview("t", "id", {
        fetchFn: server(HTML, { "content-type": type }).fn,
        makeUrl,
      });
      expect(r, type).toEqual({ none: true });
    }
    expect(made).toHaveLength(0);
  });

  it("reports a refusal or a broken connection as failed, and an abort as nothing made", async () => {
    reset();
    expect(await loadPreview("t", "id", { fetchFn: server(HTML, {}, 404).fn, makeUrl })).toEqual({
      failed: true,
    });
    expect(
      await loadPreview("t", "id", {
        fetchFn: (async () => {
          throw new TypeError("network");
        }) as unknown as typeof fetch,
        makeUrl,
      }),
    ).toEqual({ failed: true });
    const ctl = new AbortController();
    const s = server(PDF, { "content-type": "application/pdf" });
    const slow = (async (...a: Parameters<typeof fetch>) => {
      const r = await s.fn(...a);
      ctl.abort();
      return r;
    }) as typeof fetch;
    expect(await loadPreview("t", "id", { fetchFn: slow, makeUrl, signal: ctl.signal })).toEqual({
      failed: true,
    });
    expect(made).toHaveLength(0);
  });

  it("waits for a click above 8 MiB, without reading the body, and loads it when asked", async () => {
    reset();
    expect(PREVIEW_LIMIT).toBe(8 * 1024 * 1024);
    const big = PREVIEW_LIMIT + 1;
    const cancel = vi.fn(async () => {});
    const arrayBuffer = vi.fn(async () => PDF.buffer);
    const fake = (async () => ({
      ok: true,
      headers: new Headers({ "content-type": "application/pdf", "content-length": String(big) }),
      body: { cancel },
      arrayBuffer,
    })) as unknown as typeof fetch;
    expect(await loadPreview("t", "id", { fetchFn: fake, makeUrl })).toEqual({ large: big });
    expect(arrayBuffer).not.toHaveBeenCalled();
    expect(cancel).toHaveBeenCalledOnce();
    expect(made).toHaveLength(0);

    const forced = await loadPreview("t", "id", { fetchFn: fake, makeUrl, force: true });
    expect(forced).toMatchObject({ ok: { kind: "pdf" } });
    expect(arrayBuffer).toHaveBeenCalledOnce();
    expect(made).toHaveLength(1);
  });

  it("takes a file of exactly 8 MiB without asking", async () => {
    reset();
    const fake = (async () => ({
      ok: true,
      headers: new Headers({
        "content-type": "application/pdf",
        "content-length": String(PREVIEW_LIMIT),
      }),
      body: { cancel: async () => {} },
      arrayBuffer: async () => PDF.buffer,
    })) as unknown as typeof fetch;
    expect(await loadPreview("t", "id", { fetchFn: fake, makeUrl })).toMatchObject({ ok: {} });
  });

  it("does not ask the server for a thumbnail or add a route: the id goes in the one download path", async () => {
    const s = server(PDF, { "content-type": "application/pdf" });
    await loadPreview("a/b?c", "x", { fetchFn: s.fn, makeUrl: () => "blob:x" });
    await loadPreview("a/b?c", "a/b?c", { fetchFn: s.fn, makeUrl: () => "blob:x" });
    expect(s.calls[1]!.url).toBe("/api/blobs/a%2Fb%3Fc");
    expect(s.calls.every((c) => !c.url.includes("w="))).toBe(true);
  });
});

describe("what the component does with the file", () => {
  const code = (name: string) =>
    readFileSync(new URL(name, import.meta.url), "utf8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/^\s*\/\/.*$/gm, "");

  it("never writes the file into the page as markup, and opens full size with no opener", () => {
    const tsx = code("./BlobFields.tsx");
    for (const word of [
      "dangerouslySetInnerHTML",
      "srcDoc",
      "innerHTML",
      "document.write",
      "<iframe",
    ])
      expect(tsx, word).not.toContain(word);
    const open = tsx.slice(
      tsx.indexOf("filepreview-open") - 200,
      tsx.indexOf("filepreview-open") + 200,
    );
    expect(open).toContain('target="_blank"');
    expect(open).toContain('rel="noopener noreferrer"');
    expect(tsx).toContain("URL.revokeObjectURL");
    expect(tsx).toContain('type="application/pdf"');
  });

  it("previews files, not images (an image field already shows its picture)", () => {
    expect(code("./BlobFields.tsx")).toContain("!isImage && value && <FilePreview");
  });
});

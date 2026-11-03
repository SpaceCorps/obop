import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { SignLinkBody, SignLinkTable } from "./SignLinks.tsx";
import { copyText, linkGate, linkStatus, madeLink, revokeBody } from "./signLinks.ts";

async function use(lang: Lang, messages: Record<string, string> = {}) {
  provideCatalogue(lang, messages);
  await setLanguage(lang, false);
}
afterEach(() => use("en"));

const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const state = field("state", {
  kind: "select",
  options: ["draft", "sent", "signed", "declined", "expired"],
});
const fields = [field("document_id", { kind: "ref", to: "documents_document" }), state];
const row = (extra: Record<string, unknown>): Row => ({ id: "r1", ...extra });

const ORIGIN = "https://acme.example.com";
const TOKEN = "a".repeat(64);
const PATH = `/sign/req_1/${TOKEN}`;

describe("when a signing link can be made (D92)", () => {
  it("only for a sent request that points at a document", () => {
    expect(linkGate(row({ state: "sent", document_id: "d1" }), fields)).toEqual({ ok: true });
  });

  it("says why a draft, a signed or a document-less request cannot", () => {
    const draft = linkGate(row({ state: "draft", document_id: "d1" }), fields);
    expect(draft).toEqual({
      ok: false,
      reason: "Only available when State is Sent",
    });
    const signed = linkGate(row({ state: "signed", document_id: "d1" }), fields);
    expect(signed.ok).toBe(false);
    const none = linkGate(row({ state: "sent", document_id: "" }), fields);
    expect(none).toEqual({ ok: false, reason: "The request has no document to sign" });
    expect(linkGate(row({ state: "sent" }), fields).ok).toBe(false);
    expect(linkGate(row({ state: "sent", document_id: "  " }), fields).ok).toBe(false);
  });

  it("is checked by the server again; a request without a state field is not blocked early", () => {
    expect(linkGate(row({ document_id: "d1" }), [field("document_id")])).toEqual({ ok: true });
  });
});

describe("the server's answer to sign_link.create", () => {
  it("is a path that gets this page's origin", () => {
    expect(
      madeLink({ id: "x", link: PATH, expires_at: "2026-11-18T10:00:00.000000Z" }, ORIGIN),
    ).toEqual({
      href: `${ORIGIN}${PATH}`,
      expiresAt: "2026-11-18T10:00:00.000000Z",
    });
  });

  it("or the public address the instance is configured with", () => {
    const m = madeLink({ link: `https://sign.example.org${PATH}` }, ORIGIN);
    expect(m).toEqual({ href: `https://sign.example.org${PATH}`, expiresAt: "" });
  });

  it("is refused when it is not an http(s) address of a /sign/ path", () => {
    for (const link of [
      "",
      "javascript:alert(1)",
      "data:text/html,<script>alert(1)</script>",
      "/api/blobs/abc",
      `ftp://evil.example${PATH}`,
      `${PATH}?next=//evil`,
      `${PATH}#frag`,
      `https://user:pw@sign.example.org${PATH}`,
      "/signal/abc",
      42,
      null,
    ])
      expect(madeLink({ link }, ORIGIN), String(link)).toBeNull();
    expect(madeLink(null, ORIGIN)).toBeNull();
    expect(madeLink("string", ORIGIN)).toBeNull();
    expect(madeLink({}, ORIGIN)).toBeNull();
  });
});

describe("the dialog that shows the link once", () => {
  const made = { href: `${ORIGIN}${PATH}`, expiresAt: "2026-11-18T10:00:00.000000Z" };
  const html = (copied: "yes" | "no" | null = null, signer = "Ada Example") =>
    renderToStaticMarkup(
      <SignLinkBody
        made={made}
        signer={signer}
        copied={copied}
        onCopy={() => {}}
        onDone={() => {}}
      />,
    );

  it("warns that it is shown once, who can use it, what to do when it is lost, and that no email goes", () => {
    const h = html();
    expect(h).toContain("Shown once");
    expect(h).toContain("Anyone who has this link can sign for Ada Example.");
    expect(h).toContain("It cannot be shown again.");
    expect(h).toContain("If you lose it, create another link for this request.");
    expect(h).toContain("obop sends no email");
  });

  it("shows the link in a read-only field next to a Copy button and a Done button", () => {
    const h = html();
    expect(h).toMatch(
      /<input[^>]*readOnly=""[^>]*value="https:\/\/acme\.example\.com\/sign\/req_1\/a{64}"/,
    );
    expect(h).toContain(">Copy<");
    expect(h).toContain(">Done<");
    expect(h).toContain("The link works once and stops working on");
  });

  it("reads Copied after a copy and says so when the browser refused", () => {
    expect(html("yes")).toContain(">Copied<");
    expect(html("yes")).not.toContain(">Copy<");
    expect(html("no")).toContain(
      "Could not copy automatically. Select the link and copy it yourself.",
    );
    expect(html(null)).not.toContain("Could not copy");
  });

  it("escapes the name of the signer", () => {
    const h = html(null, "<img src=x onerror=alert(1)>");
    expect(h).not.toContain("<img");
    expect(h).toContain("&lt;img");
  });

  it("is written in the language of the person", async () => {
    await use("de", {
      "Shown once": "Einmal angezeigt",
      Copy: "Kopieren",
    });
    const h = html();
    expect(h).toContain("Einmal angezeigt");
    expect(h).toContain(">Kopieren<");
  });
});

describe("what happens to the link", () => {
  // The link is a secret that the server keeps only as a hash. Nothing of the dialog may keep it
  // in the browser (a store that outlives the dialog), print it, or put it in a toast.
  const source = (name: string) => readFileSync(new URL(name, import.meta.url), "utf8");

  it("is never stored, logged or put in a notification", () => {
    for (const file of ["./SignLinks.tsx", "./signLinks.ts"]) {
      const code = source(file)
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/^\s*\/\/.*$/gm, "");
      for (const word of [
        "localStorage",
        "sessionStorage",
        "indexedDB",
        "console.",
        "document.cookie",
      ])
        expect(code, `${file} uses ${word}`).not.toContain(word);
      expect(code, `${file} puts a link in a toast`).not.toMatch(
        /notify\([^)]*(made|href|r\.value|\.link)/,
      );
    }
  });

  it("is copied with the clipboard API and falls back to selecting the field", async () => {
    // No clipboard and no DOM here: the function answers false instead of throwing.
    expect(await copyText("x", null)).toBe(false);
  });
});

describe("the status of a link, worked out from its fields", () => {
  const NOW = Date.parse("2026-11-10T12:00:00Z");
  const link = (extra: Record<string, unknown>): Row => ({
    id: "f".repeat(64),
    request_id: "r1",
    expires_at: "2026-11-17T12:00:00.000000Z",
    ...extra,
  });

  it("is active until it is used, revoked or past its expiry", () => {
    expect(linkStatus(link({}), NOW)).toBe("active");
    expect(linkStatus(link({ used_at: "2026-11-09T10:00:00.000000Z" }), NOW)).toBe("used");
    expect(linkStatus(link({ revoked_at: "2026-11-09T10:00:00.000000Z" }), NOW)).toBe("revoked");
    expect(linkStatus(link({ expires_at: "2026-11-09T12:00:00.000000Z" }), NOW)).toBe("expired");
  });

  it("is used before it is revoked or expired, as the door decides", () => {
    const both = link({
      used_at: "2026-11-09T10:00:00.000000Z",
      revoked_at: "2026-11-09T11:00:00.000000Z",
      expires_at: "2026-11-01T00:00:00.000000Z",
    });
    expect(linkStatus(both, NOW)).toBe("used");
    const gone = link({
      revoked_at: "2026-11-09T11:00:00.000000Z",
      expires_at: "2026-11-01T00:00:00.000000Z",
    });
    expect(linkStatus(gone, NOW)).toBe("revoked");
  });

  it("treats empty and missing stamps as unset, and an unreadable expiry as no expiry", () => {
    expect(linkStatus(link({ used_at: "", revoked_at: null, expires_at: "" }), NOW)).toBe("active");
    expect(linkStatus(link({ expires_at: "soon" }), NOW)).toBe("active");
    expect(linkStatus({ id: "x" }, NOW)).toBe("active");
  });

  it("is revoked by an update of revoked_at alone", () => {
    expect(revokeBody("abc", new Date("2026-11-10T12:00:00.123Z"))).toEqual({
      id: "abc",
      revoked_at: "2026-11-10T12:00:00.123Z",
    });
  });
});

describe("the table of the links of a request", () => {
  const NOW = Date.parse("2026-11-10T12:00:00Z");
  const rows: Row[] = [
    { id: "a".repeat(64), request_id: "r1", expires_at: "2026-11-17T12:00:00.000000Z" },
    {
      id: "b".repeat(64),
      request_id: "r1",
      expires_at: "2026-11-17T12:00:00.000000Z",
      used_at: "2026-11-09T10:00:00.000000Z",
    },
    {
      id: "c".repeat(64),
      request_id: "r1",
      expires_at: "2026-11-17T12:00:00.000000Z",
      revoked_at: "2026-11-09T10:00:00.000000Z",
    },
  ];
  const html = (canRevoke: boolean, busy: string | null = null) =>
    renderToStaticMarkup(
      <SignLinkTable
        rows={rows}
        nowMs={NOW}
        canRevoke={canRevoke}
        busy={busy}
        onRevoke={() => {}}
      />,
    );

  it("shows each link's status, its first eight characters and never a link", () => {
    const h = html(true);
    expect(h).toContain("Active");
    expect(h).toContain("Used");
    expect(h).toContain("Revoked");
    expect(h).toContain("aaaaaaaa");
    expect(h).not.toContain("a".repeat(9));
    expect(h).not.toContain("/sign/");
  });

  it("offers Revoke only for an active link, and only to a person who may", () => {
    expect(html(true).match(/>Revoke</g)).toHaveLength(1);
    expect(html(true)).toContain('aria-label="Revoke signing link aaaaaaaa"');
    expect(html(false)).not.toContain(">Revoke<");
  });
});

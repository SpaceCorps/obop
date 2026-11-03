import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { SignLinkBody } from "./SignLinks.tsx";
import { copyText, linkGate, madeLink } from "./signLinks.ts";

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

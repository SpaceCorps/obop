import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import type { FieldMeta, Row } from "./api.ts";
import { SignLinkBody, SignLinkTable } from "./SignLinks.tsx";
import {
  canRevokeStatus,
  closeAttempt,
  copyText,
  linkGate,
  linkStatus,
  madeLink,
  revokeBody,
} from "./signLinks.ts";
import { markActionOnly, type ModuleMeta } from "./api.ts";

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
  const html = (copied: "yes" | "no" | null = null, signer = "Ada Example", leaving = false) =>
    renderToStaticMarkup(
      <SignLinkBody
        made={made}
        signer={signer}
        copied={copied}
        leaving={leaving}
        noteId="note-1"
        onCopy={() => {}}
        onDone={() => {}}
        onCloseAnyway={() => {}}
      />,
    );

  it("warns that it is shown once, who can use it, what to do when it is lost, and that no email goes", () => {
    const h = html();
    expect(h).toContain("Shown once");
    expect(h).toContain("Anyone who has this link can sign for Ada Example.");
    expect(h).toContain("It cannot be shown again.");
    expect(h).toContain("If you lose it, create another link for this request.");
    expect(h).toContain("No email is sent");
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

  it("starts with the focus on Copy, not on a Close button, and points the dialog at the warning", () => {
    const h = html();
    expect(h).toMatch(/<button[^>]*data-autofocus="true"[^>]*data-copy="true"[^>]*>Copy</);
    expect(h.match(/data-autofocus/g)).toHaveLength(1);
    expect(h).toContain('id="note-1"');
    // The warning that the dialog is described by sits inside that id.
    expect(h.indexOf('id="note-1"')).toBeLessThan(h.indexOf("Shown once"));
  });

  it("says Copied in a polite live region that is always there", () => {
    expect(html(null)).toMatch(/<span class="sr-only" role="status"><\/span>/);
    expect(html("yes")).toMatch(/<span class="sr-only" role="status">Copied<\/span>/);
  });

  it("asks before it lets the link go: a danger notice, Copy first, and a button that says what it does", () => {
    const h = html(null, "Ada Example", true);
    expect(h).toContain("Close without copying?");
    expect(h).toContain("You have not copied the link.");
    expect(h).toContain("the link is lost and you will have to make another one");
    expect(h).toContain(">Close without copying<");
    expect(h).not.toContain(">Done<");
    expect(h).toContain('class="notice danger"');
    // Copy is still the first button and still has the focus.
    expect(h.indexOf(">Copy<")).toBeLessThan(h.indexOf(">Close without copying<"));
    expect(html()).not.toContain("Close without copying");
    expect(html()).toContain(">Done<");
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

describe("closing the dialog that shows the link", () => {
  it("is refused until the link was copied: the first try asks, a second goes back, never closes", () => {
    expect(closeAttempt(false, false)).toEqual({ close: false, leaving: true });
    expect(closeAttempt(false, true)).toEqual({ close: false, leaving: false });
    // Ten impatient keypresses in a row never close it.
    let leaving = false;
    for (let i = 0; i < 10; i++) {
      const step = closeAttempt(false, leaving);
      expect(step.close).toBe(false);
      leaving = step.leaving;
    }
  });

  it("closes at once when the link was copied, also when the question was open", () => {
    expect(closeAttempt(true, false)).toEqual({ close: true, leaving: false });
    expect(closeAttempt(true, true)).toEqual({ close: true, leaving: false });
  });
});

// What the dialog does with the link, and that it is kept nowhere, is tested on the mounted dialog in
// signLinks.dialog.test.tsx; this file holds the pure helpers.
describe("copying the link", () => {
  it("is copied with the clipboard API and falls back to selecting the field", async () => {
    // No clipboard and no DOM here: the function answers false instead of throwing.
    expect(await copyText("x", null)).toBe(false);
  });
});

describe("the status of a link, worked out the way the door works it out", () => {
  const NOW = Date.parse("2026-11-10T12:00:00Z");
  const link = (extra: Record<string, unknown>): Row => ({
    id: "f".repeat(64),
    request_id: "r1",
    expires_at: "2026-11-17T12:00:00.000000Z",
    ...extra,
  });
  const req = (extra: Record<string, unknown> = {}): Row => ({
    id: "r1",
    state: "sent",
    expires_on: "",
    ...extra,
  });

  it("is active until it is used, revoked, past its end, or its request is over", () => {
    expect(linkStatus(link({}), NOW, req())).toBe("active");
    expect(linkStatus(link({ used_at: "2026-11-09T10:00:00.000000Z" }), NOW, req())).toBe("used");
    expect(linkStatus(link({ revoked_at: "2026-11-09T10:00:00.000000Z" }), NOW, req())).toBe(
      "revoked",
    );
    expect(linkStatus(link({ expires_at: "2026-11-09T12:00:00.000000Z" }), NOW, req())).toBe(
      "expired",
    );
  });

  it("is used before it is revoked or expired, and revoked before the request is looked at, as the door decides", () => {
    const both = link({
      used_at: "2026-11-09T10:00:00.000000Z",
      revoked_at: "2026-11-09T11:00:00.000000Z",
      expires_at: "2026-11-01T00:00:00.000000Z",
    });
    expect(linkStatus(both, NOW, req({ state: "declined" }))).toBe("used");
    const gone = link({
      revoked_at: "2026-11-09T11:00:00.000000Z",
      expires_at: "2026-11-01T00:00:00.000000Z",
    });
    expect(linkStatus(gone, NOW, req({ state: "signed" }))).toBe("revoked");
  });

  it("is closed when the request is signed (by another link), declined, expired or still a draft", () => {
    for (const state of ["signed", "declined", "expired", "draft"])
      expect(linkStatus(link({}), NOW, req({ state })), state).toBe("closed");
  });

  it("is closed, not expired, for a link whose request ended first, unless its own end has passed too", () => {
    // Not past its end: the request decides (door.rs, step 5).
    expect(linkStatus(link({}), NOW, req({ state: "declined" }))).toBe("closed");
    // A signed request wins over the end of the link (door.rs, step 3).
    const late = link({ expires_at: "2026-11-01T00:00:00.000000Z" });
    expect(linkStatus(late, NOW, req({ state: "signed" }))).toBe("closed");
    // Past its own end and the request merely declined: expired (step 4 comes before step 5).
    expect(linkStatus(late, NOW, req({ state: "declined" }))).toBe("expired");
  });

  it("ends with the day of expires_on of the request, in UTC, even when the link would live longer", () => {
    const l = link({ expires_at: "2026-11-17T12:00:00.000000Z" });
    const on = req({ expires_on: "2026-11-10" });
    expect(linkStatus(l, NOW, on)).toBe("active");
    expect(linkStatus(l, Date.parse("2026-11-10T23:59:59.000Z"), on)).toBe("active");
    expect(linkStatus(l, Date.parse("2026-11-11T00:00:00.000Z"), on)).toBe("expired");
    expect(linkStatus(l, NOW, req({ expires_on: "2026-11-09" }))).toBe("expired");
    // A request without an expires_on, or one that cannot be read, does not shorten the link.
    expect(linkStatus(l, NOW, req({ expires_on: "" }))).toBe("active");
    expect(linkStatus(l, NOW, req({ expires_on: "soon" }))).toBe("active");
  });

  it("treats a link without a readable end as expired, as the door does", () => {
    expect(linkStatus(link({ expires_at: "" }), NOW, req())).toBe("expired");
    expect(linkStatus(link({ expires_at: "soon" }), NOW, req())).toBe("expired");
    expect(linkStatus({ id: "x" }, NOW, req())).toBe("expired");
  });

  it("skips what needs the request when it is not given", () => {
    expect(linkStatus(link({}), NOW)).toBe("active");
    expect(linkStatus(link({ expires_at: "2026-11-09T12:00:00.000000Z" }), NOW)).toBe("expired");
    expect(linkStatus(link({ used_at: "", revoked_at: null }), NOW)).toBe("active");
  });

  it("offers Revoke for every link that is not used or revoked, so a wrong browser clock cannot hide it", () => {
    expect(canRevokeStatus("active")).toBe(true);
    expect(canRevokeStatus("expired")).toBe(true);
    expect(canRevokeStatus("closed")).toBe(true);
    expect(canRevokeStatus("used")).toBe(false);
    expect(canRevokeStatus("revoked")).toBe(false);
  });

  it("is revoked by an update of revoked_at alone", () => {
    expect(revokeBody("abc", new Date("2026-11-10T12:00:00.123Z"))).toEqual({
      id: "abc",
      revoked_at: "2026-11-10T12:00:00.123Z",
    });
  });
});

describe("the generic New of the signing links model", () => {
  const model = (name: string, can: boolean) =>
    ({
      name,
      can_create: can,
      can_bulk_create: can,
      can_update: can,
      can_delete: can,
      fields: [],
      actions: [],
    }) as unknown as ModuleMeta["models"][number];
  const meta = (models: ModuleMeta["models"]): ModuleMeta[] => [
    { id: "sign", name: "Sign", models },
  ];

  it("is not offered for a person who may make links, who still gets the right through via_action", () => {
    const m = meta([model("sign_link", true), model("sign_request", true)]);
    markActionOnly(m);
    const [link, request] = m[0]!.models;
    expect(link!.can_create).toBe(false);
    expect(link!.can_bulk_create).toBe(false);
    expect(link!.via_action).toBe(true);
    // Other models are not touched.
    expect(request!.can_create).toBe(true);
    expect(request!.can_bulk_create).toBe(true);
    expect(request!.via_action).toBeUndefined();
  });

  it("gives no right to a person who may not make links", () => {
    const m = meta([model("sign_link", false)]);
    markActionOnly(m);
    expect(m[0]!.models[0]!.via_action).toBe(false);
    expect(m[0]!.models[0]!.can_create).toBe(false);
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
        request={{ id: "r1", state: "sent", expires_on: "" }}
        nowMs={NOW}
        canRevoke={canRevoke}
        busy={busy}
        onRevoke={() => {}}
      />,
    );

  it("shows each link's status, its first eight characters and never a link", () => {
    const h = html(true);
    // The badges themselves (the moments under each link use the words Used and Revoked too).
    expect(h).toMatch(/<\/i>Active<\/span><code[^>]*>aaaaaaaa</);
    expect(h).toMatch(/<\/i>Used<\/span><code[^>]*>bbbbbbbb</);
    expect(h).toMatch(/<\/i>Revoked<\/span><code[^>]*>cccccccc</);
    expect(h).toContain("aaaaaaaa");
    expect(h).not.toContain("a".repeat(9));
    expect(h).not.toContain("/sign/");
  });

  it("offers Revoke for a link that is neither used nor revoked, and only to a person who may", () => {
    expect(html(true).match(/>Revoke</g)).toHaveLength(1);
    expect(html(true)).toContain('aria-label="Revoke signing link aaaaaaaa"');
    expect(html(false)).not.toContain(">Revoke<");
  });

  it("shows a link of a request that is over as closed, with Revoke still there", () => {
    const h = renderToStaticMarkup(
      <SignLinkTable
        rows={[rows[0]!]}
        request={{ id: "r1", state: "declined", expires_on: "" }}
        nowMs={NOW}
        canRevoke
        busy={null}
        onRevoke={() => {}}
      />,
    );
    expect(h).toContain("Request closed");
    expect(h).not.toContain(">Active<");
    expect(h).toContain(">Revoke<");
  });

  it("shows a link that is past its end as expired, and still offers Revoke", () => {
    const old: Row = { ...rows[0]!, expires_at: "2026-11-01T00:00:00.000000Z" };
    const h = renderToStaticMarkup(
      <SignLinkTable
        rows={[old]}
        request={{ id: "r1", state: "sent", expires_on: "" }}
        nowMs={NOW}
        canRevoke
        busy={null}
        onRevoke={() => {}}
      />,
    );
    expect(h).toContain(">Expired<");
    expect(h).toContain(">Revoke<");
  });
});

# Signing a Document through a link (sign, D92)

- **Status:** PROPOSED (D92), built in a draft pull request (branch `feat/sign-signer`, PR 1 of the signer work; PR 0 is the blob row gate, PR 2 the preview in the Documents record view). **Nothing here is ratified, deployed or run on a live instance.** It was tested against synthetic bytes in the repository's tests only. It has not been tried in the browser matrix at the end of this page, on a phone, or behind the production proxy.
- **Scope:** how a person in staff has a Document (a PDF kept in the Documents app) signed by one other person who has no account, through a link. For the package format see [MODULES.md](MODULES.md); for signing through an outside service see [SIGN-DOCUSEAL.md](SIGN-DOCUSEAL.md); for the decision record see D92 in [DECISIONS.md](DECISIONS.md).

## What it is

A **simple electronic signature**: the signer opens a long random link, reads the PDF, types their full name, ticks "I have read and agree" and presses Sign. obop records the typed name, the time, the address and the browser the request came from, and the SHA-256 of the exact file, in one step. There is no account, no email, no cookie and no script on the signer's page.

**The link is the only authentication.** Whoever holds the link can sign with it, and the typed name is whatever they typed. The record shows that someone who held the link typed this name, ticked the box, at this time, from this address as reported by the proxy chain and with this browser. **It does not show who that person was.** It is not an advanced or a qualified signature (D39) and it carries no trusted timestamp (the time is the server clock). Whether a simple electronic signature is enough for a given contract depends on the law that governs it; ask counsel before relying on it.

## The flow

1. **Install.** `sign` 0.2.1 needs `documents` (Q65 below). See "Upgrade runbook" for an instance that has `sign` already.
2. **Make the request.** A person with `sign.user` creates a `sign_request` with a title, the requested signer's name (`signer_name`) and the Document (`document_id`). No template and no email address are needed for a request about a Document; `expires_on` is optional.
3. **Send it.** The action `send_document` (group `sign.user`; an agent can only propose it, a person approves, D18) takes the SHA-256 of the Document's file, which is its blob id. The file must be a PDF of at most 25 MiB. obop **holds an exact copy** for the request (`document_file`, a content-addressed blob), so replacing or deleting the Document's file later cannot change what was sent. The request moves to `sent`. The action `send` of the DocuSeal path is unchanged; the document flow has its own action so that a DocuSeal connection subscribed to `sign_request.send` does not fire (it would send the signer's name to DocuSeal).
4. **Create the link.** A person with `sign.manager` creates a signing link for the request (`sign_link.create`, or the button "Create signing link" on the request in the back office). Minting a link is **the same right as recording a signature**, because whoever holds the link can sign: it needs `sign.manager`, not `sign.user`. The link is shown **once** and is never stored; only a hash of it is. Hand it over yourself (obop sends no email). The link looks like `https://<your instance>/sign/<request id>/<64 hex characters>`.
5. **The signer signs.** The page shows the request title, the requested name, the Document title, the first 16 characters of the file's SHA-256, the held PDF (the browser's own viewer, plus a link that opens it in full size) and the form. On a phone the full-size link is the way to read the whole document. One atomic step then marks the link used, marks the request `signed` with the evidence, and appends a `sign_signed` event to the security log. A second submit, with the same or another name, changes nothing.
6. **Read the evidence.** `sign_request.get` shows `signed_at`, `signed_name`, `signer_ip`, `signer_user_agent`, `document_hash` and `consent_version` to `sign.manager` on every surface (REST, MCP, CLI and the back office). The Document's related-records panel lists every request that refers to it, so two signers of one NDA are two requests.

**Several links per request are allowed.** Creating a new link does not cancel an older one: revoke an old link yourself if it may have been seen by someone it should not have been. A link is valid for 14 days by default, never longer than the end of the request's `expires_on` day, and works once.

## What is recorded

| Where | What | Notes |
|---|---|---|
| `sign_request.document_hash` | SHA-256 of the file, lowercase hex | set at send; equals the blob id |
| `sign_request.document_file` | the held copy of the PDF | readable only to people who hold the group `sign.system`, which nobody holds; an administrator satisfies every group |
| `sign_request.signed_at` | the time of signing, UTC, server clock | |
| `sign_request.signed_name` | what the signer typed | personal data; `signer_name` stays the name that was requested |
| `sign_request.signer_ip` | the address as reported by the proxy chain, or `unknown` | parsed as an IP address; see "Settings and preflight" |
| `sign_request.signer_user_agent` | the browser, cut to 300 characters | |
| `sign_request.signature_reference` | `typed:<name>` | |
| `sign_request.consent_version` | the id of the consent wording and the page language, for example `v1-draft/de` | the wording is a draft, see below |
| `sign_link` | the link's hash as its id, the request, the expiry, when it was used or revoked | the link itself is never stored |
| security log | `sign_link_issued` (who made a link and when), `sign_signed` (the evidence and a digest of it), `sign_refused` (a refused attempt on a known link, at most one per link and reason in ten minutes) | never the link, and unknown links are counted and not logged |

The typed name, the address and the browser are **personal data**. They are kept with the request. obop has no generic erasure tool for them yet and no automatic expiry (Q68). The security log entry carries a digest of the evidence at the moment of signing, written by the door; it is append-only and readable by administrators.

## What it does not do

- **It does not prove who signed.** See above. The page names the person the request is for and the consent sentence says "I am <that name>", but nothing checks it.
- **It is not tamper-proof and it does not claim to be.** After signing, a person who holds `sign.user` can still edit the request through the generic commands unless the draft's wrappers refuse it (the wrappers refuse the door-owned fields and, for a signed request, update and delete by anyone but a platform administrator; see D92). An administrator or anyone with database access can change anything. What exists is an append-only log entry written at signing, with the evidence and its digest. Nothing in this draft reads that digest back and says "matches"; do not describe the record as tamper-proof or as tamper-evident.
- **The address is only as good as the proxy.** `signer_ip` is the address that the server takes from the proxy chain. If the instance is reachable without the proxy, or the proxy does not overwrite `X-Forwarded-For`, the value is chosen by whoever sends the request. The audit event carries the peer address and the raw `X-Forwarded-For` (cut to 200 characters) next to it, so the evidence can be reviewed.
- **Approval is of the request, not of the bytes.** An agent-proposed send runs on whatever file is attached when a person approves it. The evidence records the exact bytes that were sent.
- **Staff who see the link can sign in the signer's name.** The address and browser make it visible, not impossible.
- **A request about a restricted Document is listable.** `sign_request` has no row-level access of its own, so every person with `sign.user` can list requests (title, requested name) even for a restricted Document. The Document and its bytes stay restricted (PR 0, the blob row gate).
- **Who is refused a link.** `sign_link.create` refuses agent tokens, scoped tokens and the tokens of mobile apps ("a signing link is a secret; ask a person to create it") and it is listed on MCP. A user's own session token used from MCP, the CLI or a script is **not** refused: do not say "agents never get links", say "an agent token cannot create one". It does not count as MCP parity.
- **No email, no copy for the signer, no drawn signature, no evidence page, no pdf.js.** The signer gets no copy (Q69): staff send the signed PDF by hand. The page works from typed names only; a drawn signature is a later, additive field.

## The signer's page is not a command

Every capability of obop is a typed command on the CLI, REST and MCP. The signer's act is the one exemption D92 asks the owner to accept (Q66): `GET` and `POST /sign/<request>/<token>` and `GET /sign/<request>/<token>/document.pdf` are server-rendered public routes, like the OAuth pages and the payment webhooks, and every state change behind them is one typed, audited operation with one security event. **No operator command signs for someone**, on purpose. The routes are not on `/api`, set no cookie, carry `noindex`, are disallowed in `robots.txt` and are refused as an admin prefix. If the model `sign_link` is not registered (the package is not upgraded, or was rolled back) all three routes answer 404: deploying the binary alone exposes nothing.

## Settings and preflight

Before a link goes to a real person:

1. **`OBOP_PUBLIC_URL`** is set to the external address (`https://acme.example.com`). The signer's POST is accepted only from the page's own origin; behind a TLS-terminating proxy that does not send `X-Forwarded-Proto` the derived address is wrong and every signing attempt would be refused. The link shown to staff also starts with this address.
2. **`OBOP_TRUST_PROXY=1`** (or `--trust-proxy`; the published image sets it), so that the address in the evidence is the client's and not the proxy's, and the rate limits count per client. Never set it on an instance that is reachable without the proxy.
3. **Check the recorded address on the real deployment**: send a request to the page with a forged `X-Forwarded-For` and confirm that the address stored for the request is the one your proxy appended, not the forged one. If it is not, the address in the evidence is not worth anything. The Sliplane provisioner passes no `--trust-proxy` of its own: confirm what your instance runs with.
4. **No DocuSeal connection lists `sign_request.send`** in its events, or you accept that it fires for requests that use `send` (the document flow's `send_document` does not fire it).
5. **Proxy access logs contain the link** (it is in the path). Keep them as private as the link itself.

Rate limits count per source, per link and per minute through the shared store; they fail open if the store is unavailable (the 256-bit link is the defence, not the limiter), and they need a correct `OBOP_TRUST_PROXY` to count per client.

## Upgrade runbook (an instance that already has sign 0.2.0 and documents)

Deploying the new binary changes nothing for `sign` by itself: the new model is not installed, the three routes answer 404, and a request created before keeps working. Only the blob row gate (PR 0) is active at once.

1. Read the installed version of `documents` (Customize, Modules). The bundled `sign` 0.2.1 needs `documents` 0.3 or later, below 1.
2. **Take a backup first:** `obop backup run`. An additive module update takes no automatic backup (docs/DEPLOY.md, "Migration gates").
3. `obop modules plan sign` and read it. Expected: the model `sign_link` added, five optional fields added to `sign_request`, a new dependency on `documents` (already installed), one action added to `sign_request`, no data migration, and `template_id` and `signer_email` no longer required.
4. `obop modules update sign` (an administrator; an agent can only propose it). Old requests and templates keep working; the new fields are empty.
5. Set the settings above and run the browser checks below.

**Rollback** (`obop modules rollback sign`) keeps the data and returns to 0.2.0. Requests about a Document then lack the `template_id` and `signer_email` that 0.2.0 requires, so the manual `sign` action refuses them; their held copies stay as orphaned data; every `/sign` link answers 404. An older binary against the upgraded package sees `sign_link` as a generic model: inert rows and no route.

## A link is lost or may have leaked

The link is not stored and cannot be shown again. **Create another link** for the same request (the button on the request, or `obop call sign_link create ...`) and hand over the new one. If the old one may have been seen by someone it should not have been, **revoke it** as well. A used link cannot be used again: a second submit shows "already signed" and changes nothing. A request moved to `declined` or `expired` kills its links at once.

## Manual checks before a real signer is sent a link

Only one browser was tried, and only by the author. **Spike of 2026-11-03** (a made-up one-page PDF served by a throwaway local server, Chromium 152 in the Claude desktop app's browser pane, which has Chromium's PDF viewer): the PDF showed in `<object type="application/pdf">` when the page's content security policy had **`frame-src 'self'`** (and `child-src 'self'`), and when the PDF response carried any of `frame-ancestors 'self'` alone, `default-src 'none'; frame-ancestors 'self'`, no policy at all, or even `sandbox`. **Without `frame-src` the page's `default-src 'none'` blocks the viewer** ("Framing ... violates default-src 'none'. 'frame-src' was not explicitly set"): the box stayed empty and only the fallback text and the full-size link showed. So the page's policy must name `frame-src 'self'`, and the PDF response's policy must never contain `sandbox` even though this Chromium tolerated it (other engines were not tried). The viewer filled in a second or two after the page. Not tried: Firefox, Edge, desktop Safari, an iPhone, an Android phone, the production proxy. Twenty minutes with a **made-up PDF**: Chrome, Edge, Firefox, desktop Safari, an iPhone and an Android phone. On desktop the PDF shows in the page; on both phones the full-size link opens all pages; clicking an external link inside the test PDF does not send the signing address as the referrer; the form submits and the "Signed" page shows. If a desktop browser shows an empty box, the full-size link still works and the policy in `INLINE_PDF_CSP` is loosened to `frame-ancestors 'self'` only. The first live use must use the English or German page: all other languages are machine-drafted.

## Wording and translations

The consent sentence (`consent`, id `v1-draft`) and the notice of what is recorded are **a draft for counsel, not legal advice**. The English text is the original; German, French, Polish, Finnish, Swedish and Spanish (page and package labels) are **machine-drafted and not reviewed by a native speaker**. The id of the wording is stored with each signature (`v1-draft/de`) and is not pinned by a test: the wording may still change, and a change after the real NDA went out is possible only because the stored id says which version the signer saw. Who confirms the wording is Q67.

## Questions for the owner (not rulings)

| Q | Question | Default (built that way) |
|---|---|---|
| Q65 | May `sign` depend on `documents`? | Yes. If not: a bridge package `sign_documents`, about three hours more, 24 bundled packages. `SignService` reads `document_id` generically, so the move changes the manifest and the labels |
| Q66 | Is the signer's page an accepted exemption from "every capability is a command"? | Yes, recorded in D92; the evidence is readable on every surface |
| Q67 | Who confirms the consent sentence and the six translations before the real NDA goes out? | The owner, with counsel if wanted; use the English or German page first |
| Q68 | How long is the signer's typed name, address and browser kept, and how is it erased? | Kept with the request, documented here, no automatic erasure |
| Q69 | Does the signer get a copy of the signed document? | No, staff send it by hand; the PDF route stops working at signing |

## Not built (cut for the first draft)

A drawn signature (an additive field later), the evidence page (`GET /api/sign/evidence/<request>`) and a CLI `obop sign evidence`, the signed copy for the signer, evidence per Document, any email, pdf.js (the full-size link covers phones), a row-level access rule for `sign_request`, a check that reads the evidence digest back from the log, the approver stamp on implicit-transition pre-steps, a pinned hash in approval plans, human review of translations, and counsel review of the wording. These are listed in docs/ISSUES.md, section O.

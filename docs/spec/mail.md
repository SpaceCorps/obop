# Outbound mail (`mail`)

Status: first slice built on branch `fix/w3a-m1` (2026-11-18). Everything here is PROPOSED: the owner's direction of 2026-11-17 ("support everything, make it customizeable": SMTP in Customize plus provider APIs through Connectors, everything configurable) is the brief; nothing below is ratified. This document narrows [platform.md](platform.md) 4.7 (the W1 outbound half) to what the code does, and says what is not built.

## 1. What a person sees

Customize has a **Mail** tab (administrators, and anyone who holds `mail.admin` without being an administrator: the tab is shown to a person whose `/api/meta` holds `mail_server`). It is the one screen that sets a relay's password, since the generated form never shows a field of class `secret`:

- **Mail servers.** One or more ways out. A server is either an **SMTP relay** (host, port, security `starttls`, `tls` or `none`, user name, password) or a **provider** reached through a Connectors connection (Postmark, SendGrid, Mailgun, Microsoft Graph `sendMail`, the Gmail API). Each server has its sender (from address and name), an optional reply-to address, an optional footer added to every message, and the message kinds it serves (empty: any). One server can be the default. **Send test** sends one short message to an address the administrator types and shows the server's answer.
- **Deliveries.** The log of every message: kind, recipients, subject, server, status (`queued`, `sending`, `sent`, `failed`, `cancelled`), attempts, the last error, when it was sent. Never the body. A failed message can be sent again; a queued one can be cancelled.
- **Templates.** A template has a key (the message kind it is for, such as `invoice` or `password_reset`), a language (en, de, fr, pl, fi, sv, es) and a subject and text with `{{name}}` placeholders.

The same screens are reachable through the generated record pages (`#/mail/outbound_mail`, `#/mail/mail_server`, `#/mail/mail_template`).

## 2. Models and commands

Module `mail`, built in (`crates/kernel/obop-mail`). Permission groups (Proposed names, [CONVENTIONS.md](../CONVENTIONS.md) 4): `mail.admin` (servers, templates, the log, retry, cancel, test) and `mail.sender` (may queue a message). `mail.system` holds the stored body and is held by nobody. `mail.account_admin` of platform.md stays reserved for the W2 mailboxes.

| Model | Purpose | Notes |
|---|---|---|
| `mail_server` | A way out: SMTP relay or provider connection | `password` is data class `secret`: write-only, sealed with the instance key (`OBOP_SECRET_KEY` ring, purpose `mail-server-password`), never returned on any surface |
| `outbound_mail` | The queue and the deliveries log | the body is sealed (purpose `outbound-mail:<id>`) in a field of group `mail.system` and erased once the message is sent or cancelled; no screen shows it, and only `platform.admin` (which holds every group) can read the sealed text through the API, never the plain text. `last_error` and `server_reply` (like `mail_server.last_test_result`) are personal data of class `internal`, as the addresses are: a relay's answer often repeats the recipient, so an agent reads them masked |
| `mail_template` | Subject and text per key and language | unique per key and language |

| Command | Group | What it does |
|---|---|---|
| `outbound_mail.send` | `mail.sender` | Checks the addresses, renders the template (or takes `subject` and `body`), picks the server, queues the message. Returns the id. Never sends inside the call |
| `outbound_mail.retry` | `mail.admin` | `failed` to `queued` (declared as a move of `status`, so it is offered on a failed message only), attempts from zero; refused for a test message (its text is not stored: use Send test again) |
| `outbound_mail.cancel` | `mail.admin` | `queued` to `cancelled` (a move of `status` too), body erased |
| `outbound_mail.process` | `mail.admin` | Runs the due part of the queue now (what the worker does every few seconds); answers one row per message it tried, `id` and the new `status` (`sent`, `queued` for a later attempt, `failed`) |
| `mail_server.test_send` | `mail.admin` | Sends a test message to `test_recipient` through this server now and records the answer on the server and in the log |
| generated `get`, `list`, `search`, `create`, `update`, `delete` | read `mail.admin`; `outbound_mail` has no generated `create`, `update` or `bulk_create`: they are refused for everyone and left out of `/api/meta` (`can_create` and `can_update` false) and the OpenAPI document (only `send` queues) | |

CLI: `obop call outbound_mail send to=a@example.com subject=Hi body=Text`; REST `POST /api/outbound_mail/send`; MCP tool `outbound_mail.send`.

**Agents and mail servers.** A mail server names where every later message and the relay's stored password go, so an agent (an agent token, also one scoped to `mail.admin`) only proposes `mail_server.create`, `update` and `delete`, and a person approves the plan; the same holds for `mail_template.create`, `update` and `delete`, since a template decides the text of every later message of its kind; `test_send`, `send` and `retry` need approval too. A change of the host without a new password forgets the stored password (`password_set` becomes false), so a password is never sent to a relay it was not entered for.

**Retiring a server.** A server is switched off (`enabled` false), not deleted: the generated `delete` is refused while any message of the log names the server ("still referenced by outbound_mail"), and the log keeps finished messages for the retention period (90 days by default).

**Choosing the server.** The server named in the call; else an enabled server whose kinds list the message kind; else the enabled default server; else the one enabled server when there is exactly one; else the call is refused ("no mail server is set up"). The server is fixed on the message when it is queued.

**Templates.** `template=<key>` with `language` and `vars` (a JSON object of texts and numbers). The template of that language is used, else English, else none (refused). A placeholder without a value refuses the call, so a message never leaves with `{{name}}` in it. The text is plain text (no HTML part yet).

## 3. The queue

One worker thread per server process (`Mail::start`), woken when a message is queued and every 5 seconds. Claiming is done under the cross-process storage lock `mail.queue` with a compare-and-set of the status, and a claimed message carries a lease of 5 minutes, so several server processes on one database never send one message twice in step, and a process that died leaves its messages to another after the lease. Delivery is at least once: a crash after the server accepted a message and before the status was written sends it again; the `Message-ID` is the same on every attempt (`<id@host>`).

A temporary failure (a network error, a timeout, an SMTP `4xx`, HTTP `429` or `5xx`) is tried again after 1, 5, 30 minutes, 2 and 6 hours; after the sixth attempt, or at once on a permanent failure (SMTP `5xx`, HTTP `4xx` other than `429`, a missing setting), the message is `failed` with the reason (no body, no credentials). Retention: sent and cancelled messages are removed after `OBOP_MAIL_RETENTION_DAYS` (default 90) days, failed ones too; the run is once a day under the lock `mail.purge`.

## 4. Transports

**SMTP** is written in this crate over `rustls` (already in the tree; no new dependency): `EHLO`, `STARTTLS` (required when the security is `starttls`: a server that does not offer it is refused, never fallen back to plain), implicit TLS on `tls`, `AUTH PLAIN` or `AUTH LOGIN`, `MAIL FROM`, `RCPT TO`, `DATA` with dot-stuffing. Certificates are checked against the Mozilla roots of `webpki-roots`. A user name and password are never sent over a connection without TLS, except to a loopback address. The host goes through the same checks as every outbound request of the Connectors (`net::Client::vet`): the operator's host allow-list (`egress_policy.host_allowlist`), a public address (loopback only with `OBOP_CONNECTORS_ALLOW_LOOPBACK=1`, tests and local development), and the connection goes to the address that was checked. The message is MIME 1.0, `text/plain; charset=utf-8`, base64; non-ASCII names and subjects are RFC 2047 encoded words of at most 39 bytes each, and every header is folded at spaces onto lines of at most 78 characters (RFC 5322 2.2.3), so a long German subject or sender name goes out. A line break in a given value is refused before the relay is contacted. Addresses are ASCII `local@domain` without spaces, angle brackets, commas or line breaks (international addresses are refused for now).

**Providers** are entries of the Connectors catalogue with the engine `mail` (`obop-connectors/src/catalog.rs`), maturity `generic_adapter` (tested against a local stand-in, not against the service):

| Provider id | Request | Authentication |
|---|---|---|
| `postmark` | `POST https://api.postmarkapp.com/email` (JSON) | `X-Postmark-Server-Token` (api key) |
| `sendgrid` | `POST https://api.sendgrid.com/v3/mail/send` (JSON) | bearer api key |
| `mailgun` | `POST https://api.mailgun.net/v3/<domain>/messages` (form; `region = eu` uses `api.eu.mailgun.net`) | basic `api:<key>` |
| `microsoft_graph_mail` | `POST https://graph.microsoft.com/v1.0/me/sendMail` (JSON) | OAuth 2 (`Mail.Send offline_access`), the business's own app registration |
| `gmail_send` | `POST https://gmail.googleapis.com/gmail/v1/users/me/messages/send` (the MIME message, base64url) | OAuth 2 (`gmail.send`), the business's own Google Cloud project |

An administrator connects the provider in the Connectors app as for any other provider (credentials sealed there, OAuth with PKCE, a legal basis recorded), then picks that connection on a mail server. Before each message leaves through a provider, the gate checks what the Connectors gate (`Egress::prepare`) checks for personal data: flows switched on for the instance, the connection `connected`, an active legal basis that covers the class `internal`, the agreement recorded when the instance requires one, an EU or adequate provider when the instance requires one, and the connection's caps (flows per hour and records per day, counted from the log; a message over a cap waits for a later attempt). The fields `to`, `subject` and `body` of `outbound_mail`, and `cc`, `bcc` and `reply_to` when the message has them, must be on the connection's allowlist with the treatment `allow`; otherwise nothing is sent and the message fails. The sender address and name are the mail server's settings, chosen by an administrator, and are not fields of the message. Each release is one `data_flow_log` row (`model = outbound_mail`, the fields that left, a SHA-256 of the request, never the payload), written before the request leaves; when it cannot be written nothing is sent. A refusal of the gate is logged `refused`, and a provider that does not take the message (an HTTP error or a broken connection after the release) adds a row `rejected` with its answer. SMTP relays are run by or for the business itself and are not logged as a flow.

## 5. Not built (open rows)

- **Consumers.** No part of obop calls `outbound_mail.send` yet: invoice send, helpdesk replies, Sign request mails, password reset and notifications each need their own change in the module that owns them (other streams own those files). The command is ready for them.
- **HTML mail, attachments, DKIM signing, the sender-domain checker, the blocklist and opt-out suppression, `List-Unsubscribe`** (platform.md 4.7 W1 items).
- **Amazon SES** (needs AWS Signature 4) and other providers.
- **Per company.** There is no company record yet; the choice is per message kind.
- **Template sandbox** (`minijinja`): placeholders are plain substitutions; no conditions or loops.
- **A real relay or provider.** Nothing was sent to a real server: SMTP was tested against a stand-in on loopback (plain, STARTTLS and implicit TLS with a test certificate), each provider against a fake HTTP server.
- **The log-only sink of the hosted sandbox** (platform.md 4.18).
- **"Edit in list"** on the generated Deliveries page is offered and refused by the server (`outbound_mail.update` is not allowed); New and Import are hidden.

## 6. Questions for the owner

1. A relay on a private address (an SMTP server inside the business's own network, `10.0.0.5`) is refused like any private address. Should an operator setting allow private SMTP hosts?
2. The permission group names `mail.admin` and `mail.sender`.
3. Whether `smtp` with security `none` should be offered at all outside loopback (it is now, without a password).

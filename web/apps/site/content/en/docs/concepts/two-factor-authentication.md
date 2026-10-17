---
title: "Two-factor authentication"
description: "How two-factor sign-in works in obop: a 6-digit code from an authenticator app, recovery codes, the policy that requires it for administrators, the break-glass reset and what it does not cover."
---

**Two-factor authentication** (2FA) adds a second step to signing in: after the password, the 6-digit code that an authenticator app on your phone shows. obop uses the standard time-based one-time password (TOTP, RFC 6238), so any authenticator app works: 6 digits, a new code every 30 seconds.

## Turn it on

Open the account menu, choose **Security** (it is also a tab under Customize), and select **Set up two-factor**.

1. Scan the QR code with your authenticator app, or type the key shown under it into the app. The code is drawn in your browser; nothing is sent to an image service.
2. Enter the 6-digit code the app shows. Nothing is active until a correct code is entered.
3. Save the **10 recovery codes**. They are shown once: copy them, or download them as a text file, then confirm that you saved them.

From then on, signing in asks for the password and then for a code. If you lose the phone, enter a recovery code instead (**Use a recovery code**). Each recovery code works once. When few are left the Security page says so; **New recovery codes** replaces all of them.

**Turn off two-factor** and **New recovery codes** need your password and a current code (or a recovery code).

## Administrators must have it

By default every person who holds `platform.admin` has to use two-factor on a new instance. The first administrator of a hosted instance signs in with the password they signed up with, and is taken straight to the setup: until it is done, every other request answers `403` with the code `mfa_enrolment_required` and a hint, and the app shows only the setup. Finishing it lifts that same session.

An administrator can change the policy under Customize > Security: **Require 2FA for admins** (on by default) and **Require 2FA for everyone** (off by default). The stored setting is `security.require_mfa_for_admins` and `security.require_mfa_for_all` in the instance settings. The policy applies at the next sign-in. Operators who run the test suite or a demo can change the default with `OBOP_REQUIRE_MFA_FOR_ADMINS=0`; a value an administrator stored wins.

## Agent tokens

[Agent tokens](/docs/agents/tokens/) are not people and do not use two-factor: they keep working as before and are not asked for a code. They also cannot set it up, turn it off or reset it for anyone: those routes answer `404` to an agent token. Only a signed-in person can.

## Lost phone and codes

- An **administrator** can reset another person's two-factor under Customize > Security > _Reset someone's two-factor_. The person is signed out and sets it up again. An administrator cannot reset their own that way.
- If **no administrator** can sign in, the operator uses the break-glass command with access to the database:

```bash
obop admin reset-mfa alice --db sqlite://obop.db
```

Both are written to the security log.

## The security log

Every setup step, wrong code, recovery-code use, reset, lockout and limited sign-in is appended to a log that only administrators can read. It holds the username, what happened, who did it and when, and never a code or a secret. It is the `security_event` model of the `platform` module, so it is also available to administrators over REST and MCP (`security_event.list`, `security_event.search`, `security_event.get`) and nobody can write to it through any surface.

## For the REST API

| Request                                                                                        | What it does                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /api/auth/login`                                                                         | With a password and two-factor on, answers `{"mfa_required": true, "mfa_token": "…"}` instead of a session. The token is valid for 5 minutes, allows 5 wrong codes and is not a bearer token. |
| `POST /api/auth/login/mfa`                                                                     | `{mfa_token, code}` or `{mfa_token, recovery_code}` answers `{"token": "…"}`. Wrong codes count toward the same lockout as wrong passwords.                                                   |
| `GET /api/auth/mfa`, `POST /api/auth/mfa/begin`, `/confirm`, `/disable`, `/recovery`, `/reset` | Status, setup, activation, turning off, new recovery codes and the administrator reset. A person's session only.                                                                              |

## Security of the secrets

The shared secret is stored encrypted (ChaCha20-Poly1305) under a key derived from the server secret `OBOP_SECRET_KEY`, which an operator sets as a secret environment variable. Without that variable two-factor cannot be turned on. Recovery codes are stored as hashes. A code is accepted once, so a code that was seen cannot be replayed.

## Limits

- TOTP can be phished by a site that relays the code in real time. Passkeys (WebAuthn), which resist that, are planned but not built; SMS codes are deliberately not offered.
- The server holds the key. Whoever controls both the running server and `OBOP_SECRET_KEY` can read the secrets. The key can be rotated without downtime with `obop secrets rotate` (see [Self-hosting](/docs/reference/self-hosting/)).
- There is no step-up check for sensitive actions and no "remember this browser".
- Other languages of this page are not translated yet and show the English text.

See [Permission groups](/docs/concepts/permission-groups/), [Environment variables](/docs/reference/environment/) and [Security](/docs/reference/security/).

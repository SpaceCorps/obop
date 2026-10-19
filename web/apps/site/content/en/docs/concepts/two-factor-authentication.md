---
title: "Two-factor authentication"
description: "How two-factor sign-in works in obop: a 6-digit code from an authenticator app or a passkey, recovery codes, the policy that requires it for administrators, the break-glass reset and what it does not cover."
---

**Two-factor authentication** (2FA) adds a second step to signing in: after the password, the 6-digit code that an authenticator app on your phone shows. obop uses the standard time-based one-time password (TOTP, RFC 6238), so any authenticator app works: 6 digits, a new code every 30 seconds.

## Turn it on

Open the account menu, choose **Security** (it is also a tab under Customize), and select **Set up two-factor**.

1. Scan the QR code with your authenticator app, or type the key shown under it into the app. The code is drawn in your browser; nothing is sent to an image service.
2. Enter the 6-digit code the app shows. Nothing is active until a correct code is entered.
3. Save the **10 recovery codes**. They are shown once: copy them, or download them as a text file, then confirm that you saved them.

From then on, signing in asks for the password and then for a code. If you lose the phone, enter a recovery code instead (**Use a recovery code**). Each recovery code works once. When few are left the Security page says so; **New recovery codes** replaces all of them.

**Turn off two-factor** and **New recovery codes** need your password and a current code (or a recovery code).

## Passkeys

A **passkey** is a second step that cannot be typed into a fake page: your phone, laptop or a security key proves that you hold it, bound to the address of your instance. Open **Security**, find **Passkeys** and choose **Add a passkey**. obop asks for your password again (and a second factor, if you already have one), then your browser asks you to create the passkey with your fingerprint, face, screen lock or security key.

- **As a second step.** After the password, sign-in offers **Use a passkey** next to the 6-digit code. A passkey satisfies the administrator policy above, alone or beside an authenticator app. The first second factor you add, whichever kind, also gives you the 10 recovery codes.
- **Instead of the password (optional).** Turn on **Let me sign in with a passkey alone**. Then **Sign in with a passkey** on the sign-in page, and on the sign-in page of an app that connects over MCP, needs no username and no password. It works only with a passkey that verified you (PIN, fingerprint or face) when it was made and again at sign-in. It is off until you turn it on.
- **Managing them.** The list shows each passkey's name, when it was added and last used, whether it verifies you and whether it is synced by a passkey manager. Renaming, removing and the switch above ask for your password again. Removing a passkey signs out your other sessions, and is refused when it is your last second factor and your administrator requires one.
- **Lost it.** Use a recovery code, ask an administrator to reset your two-factor, or ask the operator to run `obop admin reset-mfa`. A reset removes your passkeys too.
- **Needs an address.** Passkeys work when the instance is reached by a host name over https (or `localhost` while developing), and the operator has set `OBOP_PUBLIC_URL` behind a proxy. An IP address or plain http does not work, and Security says so. If the address of an instance changes, passkeys made for the old one stop working and people add new ones.

The server stores only the public key, a name and counters, never anything that could sign in. A counter that goes backwards (a copied passkey) is refused and logged.

## Administrators must have it

By default every person who holds `platform.admin` has to use two-factor (an authenticator app or a passkey) on a new instance. The first administrator of a hosted instance signs in with the password they signed up with, and is taken straight to the setup: until it is done, every other request answers `403` with the code `mfa_enrolment_required` and a hint, and the app shows only the setup. Finishing it lifts that same session.

An administrator can change the policy under Customize > Security: **Require 2FA for admins** (on by default) and **Require 2FA for everyone** (off by default). The stored setting is `security.require_mfa_for_admins` and `security.require_mfa_for_all` in the instance settings. The policy applies at the next sign-in. Operators who run the test suite or a demo can change the default with `OBOP_REQUIRE_MFA_FOR_ADMINS=0`; a value an administrator stored wins.

## Agent tokens

[Agent tokens](/docs/agents/tokens/) are not people and do not use two-factor: they keep working as before and are not asked for a code. They also cannot set it up, turn it off or reset it for anyone: those routes answer `404` to an agent token. Only a signed-in person can.

## Lost phone and codes

- An **administrator** can reset another person's two-factor under Customize > Security > _Reset someone's two-factor_. The person is signed out and sets it up again. An administrator cannot reset their own that way.
- If **no administrator** can sign in, the operator uses the break-glass command with access to the database:

```bash
obop admin reset-mfa alice --db sqlite://obop.db
```

Both remove passkeys as well as the authenticator app, and both are written to the security log.

## The security log

Every setup step, wrong code, passkey added, renamed, removed or used, recovery-code use, reset, lockout and limited sign-in is appended to a log that only administrators can read. It holds the username, what happened, who did it and when, and never a code, a key or a secret. It is the `security_event` model of the `platform` module, so it is also available to administrators over REST and MCP (`security_event.list`, `security_event.search`, `security_event.get`) and nobody can write to it through any surface.

## For the REST API

| Request                                                                                        | What it does                                                                                                                                                                                                                    |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /api/auth/login`                                                                         | With a password and two-factor on, answers `{"mfa_required": true, "mfa_token": "…", "methods": ["totp", "passkey"]}` instead of a session. The token is valid for 5 minutes, allows 5 wrong answers and is not a bearer token. |
| `POST /api/auth/login/mfa`                                                                     | `{mfa_token, code}` or `{mfa_token, recovery_code}` answers `{"token": "…"}`. Wrong codes count toward the same lockout as wrong passwords.                                                                                     |
| `GET /api/auth/mfa`, `POST /api/auth/mfa/begin`, `/confirm`, `/disable`, `/recovery`, `/reset` | Status, setup, activation, turning off, new recovery codes and the administrator reset. A person's session only.                                                                                                                |

| `POST /api/auth/login/passkey/options`, `POST /api/auth/login/passkey` | WebAuthn sign-in. With `{mfa_token}` it is the second step; without, a passwordless sign-in with a discoverable passkey the person allowed. The answer is a session, as above. |
| `GET /api/auth/passkeys`, `POST /api/auth/passkeys/register/options`, `/register/finish`, `/{id}/rename`, `/{id}/remove`, `/passwordless`, `/proof/options`, `/proof` | Passkey list and management. Changes need `password` and, with a second factor on, a code, a recovery code or a passkey proof given just before. A person's session only. |

## Security of the secrets

The shared secret is stored encrypted (ChaCha20-Poly1305) under a key derived from the server secret `OBOP_SECRET_KEY`, which an operator sets as a secret environment variable. Without that variable two-factor cannot be turned on. Recovery codes are stored as hashes. A code is accepted once, so a code that was seen cannot be replayed.

## Limits

- TOTP can be phished by a site that relays the code in real time; passkeys resist that, so prefer them where your devices allow. SMS codes are deliberately not offered.
- Passkeys were tested with software authenticators (Rust) and Chrome's virtual authenticator, not with real phones, security keys, Safari or Firefox. They work on instances, not on the sign-up service of the hosted platform. A passkey is made for one address; there is no sign-in hint for a non-discoverable key, so those work only as a second step; there is no attestation, so obop cannot tell makes of authenticator apart.
- The server holds the key. Whoever controls both the running server and `OBOP_SECRET_KEY` can read the secrets. The key can be rotated without downtime with `obop secrets rotate` (see [Self-hosting](/docs/reference/self-hosting/)).
- There is no step-up check for sensitive actions and no "remember this browser".
- Other languages of this page are not translated yet and show the English text.

See [Permission groups](/docs/concepts/permission-groups/), [Environment variables](/docs/reference/environment/) and [Security](/docs/reference/security/).

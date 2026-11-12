---
title: "Privacy"
description: "Draft privacy notice for the obop website and for the hosted service SpaceCorps plans, which is not offered yet: what is collected, why, how long, and your rights. A draft for review by counsel, not legal advice."
lead: "A plain-language draft of how this website, and the hosted service SpaceCorps plans, handle personal data. That service is not offered yet."
eyebrow: "Draft"
---

> **This is a draft.** It describes in plain language part of what the software does today; it is not complete. It is not legal advice and is not a final, reviewed notice. It will be replaced after review by counsel. Do not rely on it as a contract.

## Who is responsible

The obop project, run by SpaceCorps. There is no data protection officer or EU representative named yet; those details will be added in the final notice.

## This website

- The pages of this website set **no cookies** and use no analytics, advertising or third-party scripts, fonts or images. All fonts and scripts are served from this site.
- Your chosen colour theme and the dismissal of the language hint are stored in your own browser (local storage). They never leave your device.
- Like any web server, the server that hosts this site processes your IP address and the pages you request to deliver them. We do not use that to build profiles.
- The control plane software that serves these pages can also serve a sign-up app at `/app/`, a forum at `/community` and a status page at `/status`. The sentences above are about the pages of the site only.
- The sign-up app sets a cookie for your language (one year) when it opens without a language you chose earlier and without `?lang=` in the address, and again when you choose a language. It also remembers a language you choose in your browser's local storage, and keeps your sign-in token in your browser's session storage until you sign out or close the tab.
- The forum sets a cookie for your language (one year) when you ask for a language with `?lang=`, a cookie for form protection (four hours) when you open its sign-in or sign-up form while signed out and, when you sign in or sign up, a cookie for your session (30 days). A forum account is a control plane account; its display name and posts are public. Its full email address is not shown, but if you have not chosen a display name the forum makes one from the part of your email before the @. The status page lists, by name, instances that do not respond.

## The planned hosted service

SpaceCorps does not offer a hosted obop yet. When someone creates an account and an instance on a control plane, the control plane software processes at least the following:

| Data                                    | Why                                             | Where                                                     |
| --------------------------------------- | ----------------------------------------------- | --------------------------------------------------------- |
| Email address                           | Sign in, and to identify the account            | The control plane and, as your admin login, your instance |
| Password                                | Sign in. Stored only as a one-way Argon2id hash | The control plane and your instance, both as a hash       |
| Instance name and address               | To provision and show your instance             | The control plane                                         |
| Everything you enter into your instance | To run the instance                             | A database belonging to your instance                     |

We do not sell personal data. The table describes what the software does, not a service SpaceCorps offers; before the planned sandbox or hosting service opens, the final notice will say what SpaceCorps processes for it.

## Subprocessors

The hosted service SpaceCorps plans would run on Sliplane servers. The list of subprocessors and the places data is processed will be set out in the final notice and in a data processing agreement.

## How long

In the control plane software, no command deletes an account, so the account record (email address, password hash) is kept. Deleting an instance drops its database but keeps the control plane's record of it (owner email, name, address), marked deleted; backups the instance wrote are not removed.

## Your rights

Depending on where you live you may have rights to access, correct, delete, restrict or move your data and to object to processing. Until a mailbox exists, open a request through the channels on the [Contact](/contact/) page without including personal data, and we will arrange a private way to continue.

## Children

obop is not intended for children.

## Changes

This draft will change. The final notice will carry a date.

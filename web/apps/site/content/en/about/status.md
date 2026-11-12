---
title: "Status"
description: "What the status page of an obop control plane checks. SpaceCorps does not host obop for other people yet, so there is no hosted service whose status this page could report."
lead: "SpaceCorps does not host obop for other people yet, so there is no hosted service whose status to show. This page explains what the status page of a control plane checks."
eyebrow: "Status"
---

## The status page of a control plane

A control plane (`obop platform serve`) serves a status page at `/status`. It checks, from the control plane's own server:

- its own database (shown as the API and the forum);
- with the `sliplane` provisioner, that the `sliplane` tool answers;
- the website, when the control plane serves it;
- the shared PostgreSQL server, when `OBOP_CELL_ADMIN_URL` is set;
- the instances in its list that are marked as running (at most 200), both the ones it created and the ones registered with `obop platform register-instance`, by asking each one's https address (an instance whose address is not a plain `https://` host name, without a port or a path, or whose host name resolves to a private or other non-public address, is shown as not responding; so are the instances of the `local` provisioner, which have `http://127.0.0.1` addresses).

It does not check an instance that is not in its list.

## What it does not promise

There is no service level agreement, no uptime target and no support commitment. A control plane does not switch on scheduled backups of the instances it creates (the program can take them, and an instance you run yourself can turn them on).

## If something is down

1. Look at the open issues in the [repository](https://github.com/SpaceCorps/obop/issues).
2. Report it there if it is not already listed, without personal data or tokens.

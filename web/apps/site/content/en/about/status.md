---
title: "Status"
description: "Service status for the hosted obop beta: where to see whether the control plane and instances are up, and what the status page does and does not promise."
lead: "Is the hosted beta up? The live status page answers that; this page explains what it shows."
eyebrow: "Status"
---

## Live status

The live status page is at [/status](/status). It is served by the control plane itself and reports the state of the service. If it does not load, that is a sign that the control plane is down.

## What the status page covers

- The control plane: sign-up, sign-in and the instance list.
- The hosted instances the control plane provisions.

## What it does not promise

obop is in beta. There is no service level agreement, no uptime target and no support commitment yet. Instances may be restarted, upgraded by hand or lost while the platform matures; nothing backs up the hosted databases yet. Do not keep the only copy of important data in the hosted beta.

## If something is down

1. Check the [status page](/status).
2. Look at the open issues in the [repository](https://github.com/SpaceCorps/obop/issues).
3. Report it there if it is not already listed, without personal data or tokens.

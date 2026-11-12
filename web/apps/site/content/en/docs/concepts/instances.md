---
title: "Instances"
description: "An obop instance is one business: its own database, its own users and its own address. How the control plane creates instances and how self-hosting compares."
---

An **instance** is one running obop for one business. It has its own database, its own users, its own installed apps and its own address.

## The control plane

A **control plane** is a second mode of the same program (`obop platform serve`), for running instances for other people. SpaceCorps does not host obop for other people yet. A control plane does these things, among others:

1. People sign up with an email and a password.
2. Each person can create, list and delete their own instances.
3. For each instance it asks a _provisioner_ to start something.

There are three provisioners. `local` starts child `obop serve` processes with SQLite on one machine. `sliplane` creates a database on a shared PostgreSQL server and a service on Sliplane for each instance. `fake` is for tests.

The password you signed up with is copied into the new instance as an Argon2id hash, so the same email and password sign in to the instance, where you are its first administrator (`platform.admin`). Only the hash is passed on, never the password itself.

## One database per business

With the `sliplane` provisioner every business gets its own database and its own database role on a shared PostgreSQL server. An instance cannot read another instance's data, because it has no credentials for it. Records are stored as JSON documents until the schema manager exists, so column types in the database are not enforced by the database; the command layer enforces them.

## Self-hosted

Self-hosting is the same program without the control plane: `obop serve` is one instance. Use SQLite to try it and PostgreSQL (`OBOP_DATABASE_URL=postgres://...`) for real use. See [Getting started](/docs/getting-started/) and [Self-hosting and operations](/docs/reference/self-hosting/).

## Updating an instance

An administrator updates the instance from _Customize_, _Updates_. The page reads the published releases, checks every installed module against the new core, takes a backup into `OBOP_BACKUP_DIR` (where none is set, the administrator must confirm they have their own) and asks the control plane to redeploy the instance; the control plane waits for the new version to answer and, when it knows the image the instance ran before, puts that one back if it does not. The instance holds only a narrow update token, never hosting credentials, and an agent can only propose the update. A self-hosted instance gets the commands to run instead (`obop update check`). Details: [Self-hosting and operations](/docs/reference/self-hosting/).

## Known limits

- On Sliplane, an instance created from the repository (`OBOP_REPO`) builds the Rust workspace on the server, which is slow; an instance started from the published image (`OBOP_IMAGE`) needs no build.
- Upgrading the instances a control plane created (`obop platform upgrade-instances`, or _Customize_, _Updates_ in an instance) has been tested against stand-ins only, not against live Sliplane.
- The control plane does not schedule backups of the instances it creates (it does not set `OBOP_BACKUP_SCHEDULE`) and does not back up the shared database server.
- TLS ends at the hosting platform's proxy.

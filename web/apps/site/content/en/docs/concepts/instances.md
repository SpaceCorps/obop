---
title: "Instances"
description: "An obop instance is one business: its own database, its own users and its own address. How the control plane creates instances and how self-hosting compares."
---

An **instance** is one running obop for one business. It has its own database, its own users, its own installed apps and its own address.

## The control plane

The hosted service is a **control plane**, a second mode of the same program (`obop platform serve`). It does three things:

1. People sign up with an email and a password.
2. Each person can create, list and delete their own instances.
3. For each instance it asks a *provisioner* to start something.

There are three provisioners. `local` starts child `obop serve` processes with SQLite on one machine. `sliplane` creates a database on a shared PostgreSQL server and a service on Sliplane for each instance. `fake` is for tests.

The password you signed up with is copied into the new instance as a hash, so the same email and password sign in to the instance, where you are its first administrator (`platform.admin`). The control plane never sees your password after sign-up.

## One database per business

On hosted PostgreSQL every business gets its own database and its own database role on a shared server. An instance cannot read another instance's data, because it has no credentials for it. Records are stored as JSON documents until the schema manager exists, so column types in the database are not enforced by the database; the command layer enforces them.

## Self-hosted

Self-hosting is the same program without the control plane: `obop serve` is one instance. Use SQLite to try it and PostgreSQL (`OBOP_DATABASE_URL=postgres://...`) for real use. See [Getting started](/docs/getting-started/) and [Self-hosting and operations](/docs/reference/self-hosting/).

## Known limits

- Creating an instance builds the Rust workspace on the host, which is slow until a prebuilt image registry is used.
- Upgrading hosted instances is manual.
- Nothing backs up the hosted database server yet.
- TLS ends at the hosting platform's proxy.

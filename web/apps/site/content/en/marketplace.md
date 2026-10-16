---
title: "Marketplace"
description: "The obop marketplace: 22 bundled packages you can install in a click, and how to publish your own catalog as a static host with an index.json file and obop.toml packages."
lead: "Install what you need. Publish what you build. The catalog is a static host, so anyone can run one."
eyebrow: "Apps you can install"
---

## The bundled catalog

These 22 packages ship with obop (the `packages/` directory of the repository, compiled into the program). Administrators install them from _Customize_, then _Marketplace_, or with `obop modules install-from ID`. The five built-in apps (Contacts, Products, CRM, Sales and Invoicing) are not listed here because they are always present. Each name links to its app page.

%%catalog%%

Every package is checked on install: every field needs a description, a data class and a permission group, and personal data cannot be class `none`. Packages contain no code.

## Install a package

In the web app: _Customize_, _Marketplace_, choose an app, _Install_. Installing needs the administrator group.

From the command line, against a database:

```bash
obop modules market                       # list what is available
obop modules install-from helpdesk        # install one
obop modules list                         # see what is installed
```

## Publish your own marketplace

A marketplace is any HTTP or HTTPS host that serves two things:

```text
index.json                 a list of the packages
packages/<id>.toml         the obop.toml of each package listed
```

`index.json` is an array of objects with `id`, `name`, `version`, `description` and an optional `author`:

```json
[
  {
    "id": "assets",
    "name": "Assets",
    "version": "0.1.0",
    "description": "A register of company equipment with a holder and a retire step that needs approval.",
    "author": "Example Ltd"
  }
]
```

Package ids may contain only lowercase letters, digits and `_`. Point an instance at your host with the environment variable `OBOP_MARKETPLACE_URL=https://example.com/market`. The instance shows the bundled catalog first and your host after it; if the same id exists in both, the bundled one wins. Every download is validated exactly like an upload, and responses are limited to 1 MiB.

Package signing and paid listings are not built. Treat a third-party marketplace like any other software source: read the `obop.toml` before you install it. A package cannot run code or reach the network, which limits what a bad one can do, but it can still declare misleading fields or permissions.

## Write a package

A package is one text file. The [module manifest reference](/docs/reference/module-manifest/) has a full tutorial, every key and every validation error. Test it without a database:

```bash
obop modules check my-package.toml
```

## Limits today

No signing, no ratings, no paid listings, no automatic updates, and no data migrations between versions. Uninstalling keeps the data.

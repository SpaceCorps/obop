---
title: "Marketplace"
description: "The obop marketplace: bundled packages you can install in a click, and how to publish your own catalog as a static host with an index.json file and obop.toml packages."
lead: "Install what you need. Publish what you build. The catalog is a static host, so anyone can run one."
eyebrow: "Apps you can install"
---

## The bundled catalog

The packages in the `packages/` directory of the repository ship with obop, compiled into the program: 24 in all. The 22 listed here have an app page, and each name links to it. Administrators install them from _Customize_, then _Modules_, then the _Marketplace_ tab, or with `obop modules install-from ID`. The built-in apps (Contacts, Products, CRM, Sales, Invoicing, Connectors and Mileage) are not listed here because they are always present.

%%catalog%%

Every package is checked on install: every field needs a description, a data class and a permission group, and personal data cannot be class `none`. Packages contain no code.

## Install a package

In the web app: _Customize_, _Modules_, the _Marketplace_ tab, choose an app, _Install_. Installing needs the administrator group.

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

Package ids may contain only lowercase letters, digits and `_`. Point an instance at your host with the environment variable `OBOP_MARKETPLACE_URL=https://example.com/market`. The instance shows the bundled catalog first and your host after it; if the same id exists in both, the bundled one wins. Every download is validated like an upload, and it must also declare a `license` and give `requires_core` and every dependency range an upper bound (for example `^0.2`). Responses are limited to 1 MiB.

Package signing is not built. There are no paid listings, and none are planned. Treat a third-party marketplace like any other software source: read the `obop.toml` before you install it. A package cannot run code or reach the network, which limits what a bad one can do, but it can still declare misleading fields or permissions.

## Write a package

A package is one text file. The [module manifest reference](/docs/reference/module-manifest/) has a full tutorial, every key and every validation error. Test it without a database:

```bash
obop modules check my-package.toml
```

## Limits today

No signing and no ratings. There are no paid listings, and none are planned. Updates are never automatic: an administrator updates a package under _Customize_, _Modules_, _Updates_ or with `obop modules update ID`. A new version can carry `[[migration]]` data steps, and `obop modules rollback ID` can put back the version before the last update. Uninstalling keeps the data.

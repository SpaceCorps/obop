---
title: "Getting started"
description: "Run your first obop instance: self-host with Docker and PostgreSQL, or run it locally with SQLite. Then install an app, create records and invite an agent."
lead: "Two ways to run obop: Docker with PostgreSQL for a real deployment, or a local build for development. obop.dev offers no hosted instances yet."
---

SpaceCorps does not host obop for other people yet, so you run obop yourself. A free sandbox with sample data and paid hosting on dedicated servers are planned; neither is open yet, and no date is promised. See [Pricing](/pricing/).

## Option 1: self-host with Docker and PostgreSQL

You need Docker. The repository includes a Compose file that starts PostgreSQL and obop together, published on localhost only:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

The first build compiles the Rust workspace and takes a while. When it is up, open `http://127.0.0.1:8080/admin/`. The Compose file starts obop with demo data (`--demo`) and no users, so create the first administrator:

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop reads the password from standard input, so you are prompted for it (at least 12 characters). Sign in as `alice`.

To run the image against your own PostgreSQL instead, set `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

The image runs one instance by default. Its command is `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy`: the public site is at `/` and the back office at `/admin/`. `--trust-proxy` reads the client address from `X-Forwarded-For`, so run the image behind a proxy you control. Create the first administrator with `docker exec -i obop obop admin create alice --all-groups`. See [Self-hosting and operations](/docs/reference/self-hosting/) for the control plane, backups and what is missing.

## Option 2: local development with SQLite

You need the Rust toolchain from `rust-toolchain.toml`, pnpm and Vite+ (`vp`). SQLite is the default storage, so no database server is needed.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Open `http://127.0.0.1:8080` and sign in as `alice`. The data is in `obop.db` in the current directory; pass `--db sqlite:///path/to/file.db` to use another file. A new, empty SQLite database is filled with fake demo data automatically (`--demo` forces it).

`--dev-auth` swaps real sign-in for fixed tokens named `dev-admin`, `dev-sales` and `dev-agent`. It refuses to bind to anything but localhost.

## After you sign in

1. **Install an app.** Open _Customize_, then _Modules_, then the _Marketplace_ tab, and install an app such as Helpdesk or Projects. Installing and removing apps needs the administrator group. The built-in apps are already there, among them Contacts, Products, CRM, Sales, Invoicing and Connectors (the six with an app page).
2. **Create records.** Open an app, choose _New_ and fill in the form. Lists, boards and the side panel are generated from the app's description.
3. **Invite an agent.** There is no screen for agent tokens yet. Sign in over the REST API and [create an agent token](/docs/agents/tokens/), then [connect over MCP](/docs/agents/mcp/) or use [REST](/docs/agents/rest/).

## What next

- Learn the building blocks in [Concepts](/docs/concepts/).
- Browse the [apps](/apps/) and install the ones you need from _Customize_, or from the command line with `obop modules install-from helpdesk`.
- Set up an agent: [Agents overview](/docs/agents/).
- Write your own app: [module manifest reference](/docs/reference/module-manifest/).

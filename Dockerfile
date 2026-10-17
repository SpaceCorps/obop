# One image for both roles. The default command runs an instance; the control plane overrides it:
#   obop platform serve --addr 0.0.0.0:8080 --static /srv/site --provisioner sliplane
# The Rust build is split in three so the dependencies, which change rarely and take most of the
# time, are a cached layer of their own (cargo-chef): `planner` writes a recipe of every
# dependency from the manifests alone, `build` cooks the recipe (cached until a manifest or the
# lockfile changes) and then compiles the workspace.
FROM rust:1.99.0-slim AS chef
RUN apt-get update && apt-get install -y --no-install-recommends build-essential && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY packages ./packages
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p obop
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY packages ./packages
RUN cargo build --release --locked -p obop

FROM node:24-slim AS web
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web ./web
# The website (web/apps/site) is generated from these: bundled packages, the module guide, the
# ASCII hero. It has no .git here, so its changelog comes from data/changelog.snapshot.json.
COPY packages ./packages
COPY design ./design
COPY docs/MODULES.md ./docs/MODULES.md
# The website reports the core version, which is the workspace version in Cargo.toml (D55).
COPY Cargo.toml ./Cargo.toml
RUN corepack prepare pnpm@12.6.0 --activate \
 && pnpm install --frozen-lockfile \
 && OBOP_ADMIN_BASE=/admin/ pnpm --filter @obop/backoffice exec vp build \
 && pnpm --filter @obop/platform exec vp build \
 && pnpm --filter @obop/site exec vp build \
 && mkdir -p /site && cp -R web/apps/site/dist/. /site/ && cp -R web/apps/platform/dist /site/app

FROM debian:stable-slim AS cli
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
# The Sliplane CLI (SpaceCorps/Sliplane-Cli v1.0.0, linux x86_64), pinned by checksum.
RUN curl -fsSL -o /tmp/sl.tgz https://github.com/SpaceCorps/Sliplane-Cli/releases/download/v1.0.0/sliplane-v1.0.0-x86_64-unknown-linux-musl.tar.gz \
 && echo "cf63711036544a7d3232b96a2e0eba96645e9bb78260fbb0245daeac04df0999  /tmp/sl.tgz" | sha256sum -c - \
 && tar -xzf /tmp/sl.tgz -C /tmp && mv /tmp/sliplane-v1.0.0-x86_64-unknown-linux-musl/sliplane /usr/local/bin/sliplane

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=cli /usr/local/bin/sliplane /usr/local/bin/sliplane
COPY deploy/entrypoint.sh /usr/local/bin/obop-entrypoint
COPY --from=build /src/target/release/obop /usr/local/bin/obop
COPY --from=web /src/web/apps/backoffice/dist /srv/backoffice
COPY --from=web /site /srv/site
EXPOSE 8080
ENTRYPOINT ["obop-entrypoint"]
# An instance serves its public site and shop at / (a friendly page until content exists) and the
# private back office at /admin/. Sliplane terminates TLS and forwards the client address.
CMD ["obop", "serve", "--addr", "0.0.0.0:8080", "--static", "/srv/backoffice", "--public-site", "--trust-proxy"]

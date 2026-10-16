# One image for both roles. The default command runs an instance; the control plane overrides it:
#   obop platform serve --addr 0.0.0.0:8080 --static /srv/site --provisioner sliplane
FROM rust:1.99.0-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential && rm -rf /var/lib/apt/lists/*
WORKDIR /src
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
RUN corepack prepare pnpm@12.6.0 --activate \
 && pnpm install --frozen-lockfile \
 && pnpm --filter @obop/backoffice exec vp build \
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
CMD ["obop", "serve", "--addr", "0.0.0.0:8080", "--static", "/srv/backoffice"]

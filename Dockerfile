# One image for both roles. The default command runs an instance; the control plane overrides it:
#   obop platform serve --addr 0.0.0.0:8080 --static /srv/platform --provisioner sliplane
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
RUN corepack prepare pnpm@12.6.0 --activate \
 && pnpm install --frozen-lockfile \
 && pnpm --filter @obop/backoffice exec vp build \
 && pnpm --filter @obop/platform exec vp build

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/obop /usr/local/bin/obop
COPY --from=web /src/web/apps/backoffice/dist /srv/backoffice
COPY --from=web /src/web/apps/platform/dist /srv/platform
EXPOSE 8080
CMD ["obop", "serve", "--addr", "0.0.0.0:8080", "--static", "/srv/backoffice"]

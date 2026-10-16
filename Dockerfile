# Dev image: builds the backoffice with Vite+ (pnpm only) and the obop binary.
# Not production-ready: authentication is dev tokens only (see docs/DECISIONS.md D44).
FROM rust:1.99.0-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates unzip && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p obop

FROM node:26-slim AS web
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web ./web
RUN corepack prepare pnpm@12.6.0 --activate && pnpm install --frozen-lockfile && pnpm --filter @obop/backoffice exec vp build

FROM debian:stable-slim
COPY --from=build /src/target/release/obop /usr/local/bin/obop
COPY --from=web /src/web/apps/backoffice/dist /srv/web
ENV OBOP_DATABASE_URL=memory:
EXPOSE 8080
CMD ["obop", "serve", "--addr", "0.0.0.0:8080", "--static", "/srv/web", "--insecure-dev-auth"]

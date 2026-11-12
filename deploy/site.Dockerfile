# The obop website served on its own: the static files of web/apps/site and a small web server.
# No API, no sign-up app, no forum, no accounts, no database. It is what obop.dev runs while
# SpaceCorps offers no hosted obop (docs/DECISIONS.md OR-40, docs/site/SITE.md).
#
#   docker build -f deploy/site.Dockerfile -t obop-site .
#   docker run --rm -p 8080:8080 obop-site
#
# The server is deploy/site.nginx.conf; it follows crates/kernel/obop-http/src/website.rs.
FROM node:24-slim AS web
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web ./web
# The website is generated from these: bundled packages, the module guide, the ASCII hero. There
# is no .git here, so its changelog comes from data/changelog.snapshot.json.
COPY packages ./packages
COPY design ./design
COPY docs/MODULES.md ./docs/MODULES.md
# The website reports the core version, which is the workspace version in Cargo.toml (D55).
COPY Cargo.toml ./Cargo.toml
# Set OBOP_SITE_URL (build argument) when the site is served from another origin than obop.dev:
# canonical links, the sitemap and the social cards use it.
ARG OBOP_SITE_URL=https://obop.dev
ENV OBOP_SITE_URL=${OBOP_SITE_URL}
# No forum here: the sitemap index and robots.txt must not name /community/sitemap.xml.
ENV OBOP_SITE_FORUM=0
RUN corepack prepare pnpm@12.6.0 --activate \
 && pnpm install --frozen-lockfile --filter '@obop/site...' \
 && pnpm --filter @obop/site exec vp build

FROM nginxinc/nginx-unprivileged:1.30.6-alpine3.24
COPY deploy/site.nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=web /src/web/apps/site/dist /usr/share/nginx/html
# Apache-2.0 sections 4(a) and 4(d): the licence and the attribution notice travel with the image.
COPY --chmod=0644 LICENSE NOTICE /usr/share/doc/obop/
EXPOSE 8080

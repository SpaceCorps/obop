#!/bin/sh
# Builds the public website (web/apps/site: static, multi-page, localised) and the sign-up app and
# assembles them as web/site: the website at the root, the platform app under /app/.
# The control plane serves web/site with --static.
set -e
cd "$(dirname "$0")/.."
(cd web/apps/site && vp build)
(cd web/apps/platform && vp build)
rm -rf web/site
mkdir -p web/site
cp -R web/apps/site/dist/. web/site/
cp -R web/apps/platform/dist web/site/app
echo "site assembled in web/site"

#!/bin/sh
# Builds the landing page and the sign-up app and assembles them as web/site:
# the landing page at the root, the platform app under /app/.
set -e
cd "$(dirname "$0")/.."
(cd web/apps/landing && vp build)
(cd web/apps/platform && vp build)
rm -rf web/site
mkdir -p web/site
cp -R web/apps/landing/dist/. web/site/
cp -R web/apps/platform/dist web/site/app
echo "site assembled in web/site"

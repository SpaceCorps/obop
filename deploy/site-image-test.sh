#!/bin/sh
# Requests against the website image (deploy/site.Dockerfile): the behaviour the website needs from
# its server (docs/site/SITE.md "Server requirements"). Run it against a built image:
#
#   docker build -f deploy/site.Dockerfile -t obop-site .
#   deploy/site-image-test.sh obop-site
#
# It starts the image on a free local port, makes the requests, stops it, and exits non-zero when
# one answer is not what the website needs. It needs docker and curl; it changes nothing else.
set -u
IMAGE="${1:-obop-site}"
PORT="${2:-18080}"
NAME="obop-site-test-$$"
fail=0
pass=0

docker run -d --rm --name "$NAME" -p "$PORT:8080" "$IMAGE" >/dev/null || exit 2
trap 'docker stop "$NAME" >/dev/null 2>&1' EXIT INT TERM
base="http://127.0.0.1:$PORT"
i=0
until curl -s -o /dev/null "$base/" 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -gt 50 ] && { echo "the server did not start"; docker logs "$NAME" | tail -20; exit 2; }
  sleep 0.2
done

# req METHOD PATH [curl args...]: the status line and headers (lower-cased) in $H, the body in $B.
req() {
  m="$1"; p="$2"; shift 2
  out=$(curl -s -i -X "$m" "$@" "$base$p" | tr -d '\r\000')
  H=$(printf '%s\n' "$out" | sed -n '1,/^$/p' | tr 'A-Z' 'a-z')
  B=$(printf '%s\n' "$out" | sed '1,/^$/d')
}
status() { printf '%s\n' "$H" | head -1 | cut -d' ' -f2; }
header() { printf '%s\n' "$H" | grep -i "^$1:" | head -1 | cut -d' ' -f2- ; }
ok() { pass=$((pass + 1)); }
bad() { fail=$((fail + 1)); echo "FAIL: $1"; }
is() { [ "$2" = "$3" ] && ok || bad "$1: got '$2', want '$3'"; }
has() { printf '%s' "$2" | grep -qi -- "$3" && ok || bad "$1: '$3' not in '$(printf '%s' "$2" | head -c 160)'"; }
lacks() { printf '%s' "$2" | grep -qi -- "$3" && bad "$1: '$3' found" || ok; }

# Pages
req GET /
is "home status" "$(status)" 200
has "home type" "$(header content-type)" "text/html; charset=utf-8"
has "home nosniff" "$(header x-content-type-options)" nosniff
has "home vary" "$(header vary)" accept
has "home cache" "$(header cache-control)" "max-age=300, must-revalidate"
has "home body" "$B" "<h1"
req GET /privacy/
is "page status" "$(status)" 200
req GET "/privacy?x=1"
is "slashless status" "$(status)" 301
is "slashless location" "$(header location)" "/privacy/?x=1"
req GET /privacy/ -H "Accept-Encoding: gzip"
is "gzip" "$(header content-encoding)" gzip

# A real 404 with the site's own page
req GET /no-such-page/
is "404 status" "$(status)" 404
has "404 type" "$(header content-type)" "text/html"
has "404 body" "$B" "<h1"
is "404 cache" "$(header cache-control)" no-store

# Nothing of a control plane is here
for p in /app/ /community/ /status/ /api/platform/signup /openapi.json; do
  req GET "$p"
  is "$p is not served" "$(status)" 404
done
req POST /api/platform/signup -d '{}'
is "POST refused" "$(status)" 405
has "POST allow" "$(header allow)" "GET, HEAD"

# Markdown for agents
req GET /privacy/ -H "Accept: text/markdown"
is "markdown status" "$(status)" 200
is "markdown type" "$(header content-type)" "text/markdown; charset=utf-8"
has "markdown vary" "$(header vary)" accept
has "markdown body" "$B" "^title:"
req GET / -H "Accept: text/markdown"
has "home markdown type" "$(header content-type)" "text/markdown"
req GET /de/ -H "Accept: text/markdown"
has "de markdown" "$B" 'lang: "de"'
req GET /privacy/ -H "Accept: text/html,text/markdown"
has "html first stays html" "$(header content-type)" "text/html"
req GET /privacy/ -H "Accept: */*"
has "any stays html" "$(header content-type)" "text/html"
req GET /robots.txt -H "Accept: text/markdown"
has "a file is not rewritten" "$(header content-type)" "text/plain"
req GET /privacy.md
is "mirror status" "$(status)" 200
is "mirror type" "$(header content-type)" "text/markdown; charset=utf-8"

# Files
req GET /robots.txt
is "robots type" "$(header content-type)" "text/plain; charset=utf-8"
lacks "robots has no forum sitemap" "$B" "community"
has "robots names the sitemap" "$B" "^Sitemap: https://"
req GET /sitemap.xml
is "sitemap status" "$(status)" 200
has "sitemap type" "$(header content-type)" "xml"
lacks "sitemap has no forum" "$B" "community/"
req GET /llms.txt
is "llms type" "$(header content-type)" "text/plain; charset=utf-8"
req GET /.well-known/security.txt
is "security.txt status" "$(status)" 200
has "security.txt type" "$(header content-type)" "text/plain"

# Fingerprinted assets are cached for a year
css=$(curl -s "$base/" | grep -o '/assets/[^"]*\.css' | head -1)
if [ -n "$css" ]; then
  req GET "$css"
  is "asset status" "$(status)" 200
  has "asset cache" "$(header cache-control)" "max-age=31536000, immutable"
else
  bad "no /assets/*.css link on the home page"
fi

echo "passed $pass, failed $fail"
[ "$fail" -eq 0 ]

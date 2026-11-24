#!/usr/bin/env bash
# Fetches the e-invoice sample sets that obop does not commit (testdata/e-invoices/README.md): the
# CEN EN 16931 examples (EUPL-1.2) and the OpenPeppol BIS Billing 3.0 examples (licence unclear), at
# the commits pinned in testdata/e-invoices/fetch.txt, into target/e-invoice-samples/<set>/<path>.
# Every file is checked against its SHA-256 before it is kept; a mismatch stops the script and
# keeps nothing of that file. A file already there with the right hash is not fetched again.
# CI runs it before `cargo test` with OBOP_REQUIRE_EINVOICE_SAMPLES=1, so a missing file fails there;
# locally the tests that need these files skip and say how to get them.
#
#   deploy/fetch-einvoice-samples.sh                                   # into target/e-invoice-samples
#   OBOP_EINVOICE_SAMPLES_DIR=/abs/dir deploy/fetch-einvoice-samples.sh  # elsewhere (the tests read the same variable)
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
dest=${OBOP_EINVOICE_SAMPLES_DIR:-$root/target/e-invoice-samples}
list=$root/testdata/e-invoices/fetch.txt

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

total=0
fetched=0
# The path is the last field and may contain spaces; `read` gives it the rest of the line.
while read -r set repo commit want src; do
  case "$set" in '' | '#'*) continue ;; esac
  total=$((total + 1))
  file="$dest/$set/$src"
  if [ -f "$file" ] && [ "$(sha256 "$file")" = "$want" ]; then
    continue
  fi
  mkdir -p "$(dirname "$file")"
  part="$file.part"
  curl -fsSL --retry 3 --proto '=https' "https://raw.githubusercontent.com/$repo/$commit/${src// /%20}" -o "$part"
  have=$(sha256 "$part")
  if [ "$have" != "$want" ]; then
    rm -f -- "$part"
    echo "fetch-einvoice-samples: $set $src has SHA-256 $have, expected $want; nothing kept" >&2
    exit 1
  fi
  mv -- "$part" "$file"
  fetched=$((fetched + 1))
done <"$list"
echo "e-invoice samples: $total files in $dest ($fetched fetched now)"

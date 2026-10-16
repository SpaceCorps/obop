#!/bin/sh
# Registers a Sliplane API key (a secret env var set on the service) with the bundled CLI, then
# runs the given command. Without the key the control plane still starts, but provisioning
# fails with a clear error.
set -e
if [ -n "${SLIPLANE_API_KEY:-}" ]; then
  export SLIPLANE_ALLOW_PLAINTEXT_STORE=1 SLIPLANE_SECRET_STORE=plaintext
  printf %s "$SLIPLANE_API_KEY" | sliplane accounts add "${SLIPLANE_ACCOUNT:-control}" --api-key-stdin --force >/dev/null
  unset SLIPLANE_API_KEY
fi
exec "$@"

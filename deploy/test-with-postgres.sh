#!/usr/bin/env bash
# Runs the Rust tests against a throwaway PostgreSQL 18 container, including the tests that start
# two real `obop serve` processes on one database (docs/DEPLOY.md, "More than one process").
# The container and its data are removed when the script ends.
#
#   deploy/test-with-postgres.sh                      # cargo test, everything
#   deploy/test-with-postgres.sh -p obop --test multi_process   # arguments go to cargo test
set -euo pipefail
name="obop-pg-test-$$"
port="${OBOP_PG_PORT:-55432}"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$name" -e POSTGRES_PASSWORD=obop-test -p "$port:5432" postgres:18 >/dev/null
for _ in $(seq 1 60); do
  docker exec "$name" pg_isready -U postgres >/dev/null 2>&1 && break
  sleep 1
done
export OBOP_TEST_POSTGRES_URL="postgres://postgres:obop-test@127.0.0.1:$port/postgres"
cargo test "$@"

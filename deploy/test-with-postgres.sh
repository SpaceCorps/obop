#!/usr/bin/env bash
# Runs the Rust tests against a throwaway PostgreSQL 18 container, including the tests that start
# two real `obop serve` processes on one database (docs/DEPLOY.md, "More than one process").
# The container and its data (including the image's anonymous data volume) are removed when the script ends.
#
# The tests run with cargo-nextest (`cargo nextest run`, profile "default" of .config/nextest.toml);
# the doc-tests, which nextest does not run, follow with `cargo test --doc` when no argument narrows
# the run.
#
#   deploy/test-with-postgres.sh                      # nextest, then the doc-tests: everything
#   deploy/test-with-postgres.sh -p obop --test multi_process   # arguments go to cargo nextest run
set -euo pipefail
name="obop-pg-test-$$"
port="${OBOP_PG_PORT:-55432}"
cleanup() { docker rm -f -v "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$name" -e POSTGRES_PASSWORD=obop-test -p "$port:5432" postgres:18 >/dev/null
for _ in $(seq 1 60); do
  docker exec "$name" pg_isready -U postgres >/dev/null 2>&1 && break
  sleep 1
done
export OBOP_TEST_POSTGRES_URL="postgres://postgres:obop-test@127.0.0.1:$port/postgres"
# Docker is here, so the TLS tests that start a postgres:17 container of their own run too.
export OBOP_TEST_DOCKER_TLS=1
cargo nextest run "$@"
if [ "$#" -eq 0 ]; then
  cargo test --doc
fi

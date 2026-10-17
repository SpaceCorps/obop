#!/usr/bin/env bash
# Restore drill (B4, D59): prove that a backup can be restored, and that what comes back works.
# Restores BACKUP into a scratch PostgreSQL database, checks it, starts a server on it, asks it a
# question, and drops the scratch database again. Run it after every change to how you back up and
# on a schedule: a backup nobody has restored is a hope, not a backup.
#
#   DRILL_ADMIN_URL=postgres://postgres:PASSWORD@HOST:5432/postgres \
#   OBOP_SECRET_KEY=... deploy/restore-drill.sh path/to/obop-backup-....jsonl.gz
#
# DRILL_ADMIN_URL   a role that may CREATE and DROP DATABASE (required)
# OBOP_SECRET_KEY   the key the instance really uses (and OBOP_SECRET_KEY_PREVIOUS): without it
#                   the drill cannot show that the sealed secrets open
# OBOP              the obop binary (default: obop on the PATH)
# PSQL              the psql client (default: psql)
# DRILL_PORT        port for the smoke server (default 18999); DRILL_SERVE=0 skips the server
# DRILL_MIGRATE=1   run `obop migrate run` on the restored copy when the backup is from an older
#                   schema (the drill of an upgrade); otherwise the drill reports it and stops
#
# Exit status 0 only when every check passed. The scratch database is dropped either way.
set -euo pipefail

backup="${1:-}"
[ -n "$backup" ] || { sed -n '2,19p' "$0"; exit 2; }
: "${DRILL_ADMIN_URL:?set DRILL_ADMIN_URL to a postgres:// URL that may create databases}"
obop="${OBOP:-obop}"
psql="${PSQL:-psql}"
port="${DRILL_PORT:-18999}"

name="obop_drill_$(date +%s)_$$"
scratch="${DRILL_ADMIN_URL%/*}/$name"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  $psql "$DRILL_ADMIN_URL" -qAt -c "DROP DATABASE IF EXISTS $name WITH (FORCE)" >/dev/null 2>&1 || true
}
trap cleanup EXIT
fail() { echo "DRILL FAILED: $*" >&2; exit 1; }

echo "1/6 verify the backup file"
report="$("$obop" backup verify "$backup")" || fail "the backup does not verify"
echo "$report" | head -1
expected="$(echo "$report" | sed -n '1s/.*, \([0-9][0-9]*\) records.*/\1/p')"
[ -n "$expected" ] || fail "could not read the record count from: $report"

echo "2/6 create the scratch database $name"
$psql "$DRILL_ADMIN_URL" -qAt -c "CREATE DATABASE $name" >/dev/null || fail "cannot create the scratch database"

echo "3/6 restore into it"
"$obop" restore --db "$scratch" --from "$backup" || fail "the restore failed"

echo "4/6 schema version"
status="$("$obop" migrate status --db "$scratch")"
echo "$status"
if echo "$status" | grep -q "pending"; then
  if [ "${DRILL_MIGRATE:-0}" = "1" ]; then
    "$obop" migrate run --db "$scratch" || fail "migration of the restored copy failed"
  else
    fail "the backup is from an older schema; rerun with DRILL_MIGRATE=1 to drill the migration as well"
  fi
fi
echo "$status" | grep -q "NEWER" && fail "the backup is newer than this obop"

echo "5/6 smoke queries"
actual="$($psql "$scratch" -qAt -c "SELECT count(*) FROM obop_record")"
[ "$actual" = "$expected" ] || fail "the restored database holds $actual records, the backup $expected"
users="$($psql "$scratch" -qAt -c "SELECT count(*) FROM obop_record WHERE model = 'auth_user'")"
[ "$users" -gt 0 ] || fail "no user accounts came back"
echo "  $actual records, $users user account(s)"
secrets="$("$obop" secrets status --db "$scratch")"
echo "$secrets" | sed 's/^/  /'
if echo "$secrets" | grep -q "cannot be opened"; then
  fail "some sealed secrets do not open with the keys given (OBOP_SECRET_KEY, OBOP_SECRET_KEY_PREVIOUS)"
fi

if [ "${DRILL_SERVE:-1}" = "1" ]; then
  echo "6/6 start a server on the copy and ask it a question"
  "$obop" serve --addr "127.0.0.1:$port" --db "$scratch" >"${TMPDIR:-/tmp}/obop-drill-$$.log" 2>&1 &
  server_pid=$!
  for _ in $(seq 1 60); do
    code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/api/openapi.json" 2>/dev/null || true)"
    [ "$code" = "200" ] && break
    kill -0 "$server_pid" 2>/dev/null || fail "the server stopped: $(tail -3 "${TMPDIR:-/tmp}/obop-drill-$$.log")"
    sleep 0.5
  done
  [ "${code:-}" = "200" ] || fail "the server did not answer 200 (last: ${code:-none})"
  echo "  GET /api/openapi.json -> 200"
  rm -f "${TMPDIR:-/tmp}/obop-drill-$$.log"
else
  echo "6/6 (server check skipped)"
fi
echo "DRILL OK: $backup restores, verifies, opens and serves."

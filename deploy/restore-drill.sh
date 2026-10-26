#!/usr/bin/env bash
# Restore drill (B4, D59, D73): prove that a backup can be restored, and that what comes back works.
# Restores BACKUP into a scratch PostgreSQL database, checks it, starts a server on it, asks it a
# question, and drops the scratch database again. Run it after every change to how you back up and
# on a schedule: a backup nobody has restored is a hope, not a backup.
#
#   DRILL_ADMIN_URL=postgres://postgres:PASSWORD@HOST:5432/postgres \
#   OBOP_BACKUP_KEY=... OBOP_SECRET_KEY=... deploy/restore-drill.sh path/to/obop-backup-....obbk
#
# DRILL_ADMIN_URL   a role that may CREATE and DROP DATABASE (required)
# OBOP_BACKUP_KEY   the key a sealed (format 2) backup is opened with; OBOP_SECRET_KEY works when the
#                   backup was sealed with the server key. A sealed backup without its key fails the
#                   drill (exit 3, restore_key_required). Format 1 backups need neither.
# OBOP_SECRET_KEY   the key the instance really uses (and OBOP_SECRET_KEY_PREVIOUS): without it the
#                   drill cannot show that the sealed secrets open
# OBOP              the obop binary (default: obop on the PATH)
# PSQL              the psql client (default: psql)
# DRILL_PORT        port for the smoke server (default 18999); DRILL_SERVE=0 skips the server
# DRILL_ALLOW_PLAIN=1  drill a plain (format 1) backup although a key is configured; without it such a
#                   backup is refused (exit 4, backup_not_sealed): a plain pair proves nothing about who
#                   wrote it
# DRILL_MIGRATE=1   run the pending migration steps on the restored copy when the backup is from an
#                   older schema (the drill of an upgrade); otherwise the drill fails and says so
# DRILL_RECORD_IN   a database URL (the real one) to record the result in, with the measured recovery
#                   point and time; `obop storage check` and /metrics read it. It is recorded at the
#                   END, after the server on the copy has answered (or failed to): a drill whose last
#                   check failed is in the evidence as FAILED, never as a pass
#
# The checks themselves are `obop drill` (counts, a digest of every record, schema, ledger, accounts,
# sealed secrets, files, a read/write set). This script adds what needs a process: a server on the
# copy. Exit status 0 only when every check passed. The scratch database is dropped either way.
set -euo pipefail

backup="${1:-}"
[ -n "$backup" ] || { sed -n '2,28p' "$0"; exit 2; }
: "${DRILL_ADMIN_URL:?set DRILL_ADMIN_URL to a postgres:// URL that may create databases}"
obop="${OBOP:-obop}"
psql="${PSQL:-psql}"
port="${DRILL_PORT:-18999}"

name="obop_drill_$(date +%s)_$$"
scratch="${DRILL_ADMIN_URL%/*}/$name"
server_pid=""
result_file="${TMPDIR:-/tmp}/obop-drill-result-$$.json"
recorded=0
# Record the finished drill (the checks of `obop drill` plus the server check) in the database
# that DRILL_RECORD_IN names. `$1` is ok, skipped or failed; `$2` says why when it failed.
record() {
  [ -n "${DRILL_RECORD_IN:-}" ] && [ -f "$result_file" ] && [ "$recorded" = 0 ] || return 0
  recorded=1
  "$obop" drill --record-from "$result_file" --server "$1" --detail "${2:-}" --record-in "$DRILL_RECORD_IN" || true
}
cleanup() {
  rm -f "$result_file"
  if [ -n "$server_pid" ]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  $psql "$DRILL_ADMIN_URL" -qAt -c "DROP DATABASE IF EXISTS $name WITH (FORCE)" >/dev/null 2>&1 || true
}
trap cleanup EXIT
fail() { record failed "$*"; echo "DRILL FAILED: $*" >&2; exit 1; }

echo "1/4 verify the backup file (sealed backups are opened and authenticated with the key)"
set +e
report="$("$obop" backup verify "$backup" ${DRILL_ALLOW_PLAIN:+--allow-plain} 2>&1)"
code=$?
set -e
if [ "$code" -eq 4 ]; then
  echo "DRILL FAILED: the backup is plain and a key is configured (backup_not_sealed); set DRILL_ALLOW_PLAIN=1 to vouch for it" >&2
  echo "$report" >&2
  exit 4
fi
if [ "$code" -eq 3 ]; then
  echo "DRILL FAILED: the backup is sealed and its key is not configured (restore_key_required)" >&2
  echo "$report" >&2
  exit 3
fi
[ "$code" -eq 0 ] || fail "the backup does not verify: $report"
echo "$report" | head -1

echo "2/4 create the scratch database $name"
$psql "$DRILL_ADMIN_URL" -qAt -c "CREATE DATABASE $name" >/dev/null || fail "cannot create the scratch database"

echo "3/4 restore into it and check the copy (obop drill)"
args=("$backup" --scratch "$scratch" --keep)
[ -n "${DRILL_ALLOW_PLAIN:-}" ] && args+=(--allow-plain)
[ "${DRILL_MIGRATE:-0}" = "1" ] && args+=(--migrate)
# The result is kept in a file and recorded after the server step, so that one drill is one record.
[ -n "${DRILL_RECORD_IN:-}" ] && args+=(--record-later "$result_file")
"$obop" drill "${args[@]}" || fail "a check of the restored copy failed (above)"

if [ "${DRILL_SERVE:-1}" = "1" ]; then
  echo "4/4 start a server on the copy and ask it a question"
  # The copy must not take backups of itself or write into the volumes of the instance it came from.
  env -u OBOP_BACKUP_SCHEDULE -u OBOP_BACKUP_DIR -u OBOP_BACKUP_COPY_DIR \
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
  ready="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/readyz" 2>/dev/null || true)"
  [ "$ready" = "200" ] || fail "GET /readyz answered $ready, not 200"
  echo "  GET /readyz -> 200"
  rm -f "${TMPDIR:-/tmp}/obop-drill-$$.log"
  record ok
else
  echo "4/4 (server check skipped)"
  record skipped
fi
echo "DRILL OK: $backup restores, verifies, opens and serves."

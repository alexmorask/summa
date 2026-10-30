#!/usr/bin/env bash
# Proves a dump is usable: restores it into a throwaway database and checks that
# the transaction count matches the .count file and that debits equal credits.
#
#   restore-check.sh [dump]   (default: newest dump in $SUMMA_BACKUP_DIR)
#
#   SUMMA_BACKUP_DIR  where dumps live (default: $HOME/.summa/backups)
set -euo pipefail

SUMMA_BACKUP_DIR="${SUMMA_BACKUP_DIR:-$HOME/.summa/backups}"
# Make paths absolute relative to the caller's directory, before we cd.
abs_path() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$PWD/$1" ;;
  esac
}
SUMMA_BACKUP_DIR="$(abs_path "$SUMMA_BACKUP_DIR")"
if [[ $# -ge 1 ]]; then
  set -- "$(abs_path "$1")"
fi

cd "$(dirname "${BASH_SOURCE[0]}")/.."
CHECK_DB=summa_restore_check

if [[ $# -ge 1 ]]; then
  dump="$1"
else
  shopt -s nullglob
  dumps=("$SUMMA_BACKUP_DIR"/summa-*.dump)
  if ((${#dumps[@]} == 0)); then
    echo "restore check FAILED: no dumps in $SUMMA_BACKUP_DIR" >&2
    exit 1
  fi
  dump="${dumps[${#dumps[@]}-1]}"
fi

count_file="${dump%.dump}.count"
if [[ ! -f "$dump" ]]; then
  echo "restore check FAILED: dump not found: $dump" >&2
  exit 1
fi
if [[ ! -f "$count_file" ]]; then
  echo "restore check FAILED: count file missing: $count_file" >&2
  exit 1
fi

psql_check() {
  docker compose exec -T db psql -U summa -d "$CHECK_DB" -At "$@"
}
psql_admin() {
  docker compose exec -T db psql -U summa -d postgres -v ON_ERROR_STOP=1 "$@"
}

cleanup() {
  psql_admin -q -c "DROP DATABASE IF EXISTS $CHECK_DB WITH (FORCE)" >/dev/null || true
}
trap cleanup EXIT

psql_admin -q -c "DROP DATABASE IF EXISTS $CHECK_DB WITH (FORCE)"
psql_admin -q -c "CREATE DATABASE $CHECK_DB"

if ! docker compose exec -T db pg_restore --no-owner -U summa -d "$CHECK_DB" < "$dump"; then
  echo "restore check FAILED: pg_restore reported errors for $dump" >&2
  exit 1
fi

expected="$(tr -d '[:space:]' < "$count_file")"
if [[ ! "$expected" =~ ^[0-9]+$ ]]; then
  echo "restore check FAILED: count file is not a number: '$expected'" >&2
  exit 1
fi

actual="$(psql_check -c 'SELECT count(*) FROM transactions')"
if [[ "$actual" != "$expected" ]]; then
  echo "restore check FAILED: restored $actual transactions, expected $expected (a transaction may have landed between the count and the dump)" >&2
  exit 1
fi

entry_count="$(psql_check -c 'SELECT count(*) FROM entries')"
if [[ "$entry_count" == "0" && "$actual" != "0" ]]; then
  echo "restore check FAILED: restored $actual transactions but 0 entries" >&2
  exit 1
fi

# Cents are summed as integers; to_char on a numeric is exact (no floats).
IFS='|' read -r debits credits dollars < <(psql_check -c "
  SELECT
    coalesce(sum(amount) FILTER (WHERE side = 'debit'), 0),
    coalesce(sum(amount) FILTER (WHERE side = 'credit'), 0),
    to_char(coalesce(sum(amount) FILTER (WHERE side = 'debit'), 0) / 100.0, 'FM999999999990.00')
  FROM entries")
if [[ "$debits" != "$credits" ]]; then
  echo "restore check FAILED: debits ($debits cents) do not equal credits ($credits cents)" >&2
  exit 1
fi

echo "restore check passed: $actual transactions, debits = credits = \$$dollars"

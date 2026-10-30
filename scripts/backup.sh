#!/usr/bin/env bash
# Local backup of the ledger database: a pg_dump snapshot, plus a restore check.
#
#   SUMMA_DB          database to back up        (default: summa)
#   SUMMA_BACKUP_DIR  where dumps are written    (default: $HOME/.summa/backups)
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

SUMMA_DB="${SUMMA_DB:-summa}"
SUMMA_BACKUP_DIR="${SUMMA_BACKUP_DIR:-$HOME/.summa/backups}"
KEEP=7

mkdir -p "$SUMMA_BACKUP_DIR"

stamp="$(date -u +%Y-%m-%dT%H%M%SZ)"
dump="$SUMMA_BACKUP_DIR/summa-$stamp.dump"
count_file="${dump%.dump}.count"
tmp_dump="$dump.partial"
tmp_count="$count_file.partial"

# A failed run must not leave a partial file that looks like a backup.
trap 'rm -f "$tmp_dump" "$tmp_count"' EXIT

# The count is taken just before the dump, in a separate session, because pg_dump's
# snapshot cannot be read from outside it. If a transaction lands between the two,
# the restored database has one more row than the count file and the restore check
# fails loudly. That is acceptable: the check errs on the side of a false alarm,
# never a silent pass.
docker compose exec -T db psql -U summa -d "$SUMMA_DB" -At \
  -c 'SELECT count(*) FROM transactions' > "$tmp_count"

docker compose exec -T db pg_dump -U summa -Fc "$SUMMA_DB" > "$tmp_dump"

mv "$tmp_dump" "$dump"
mv "$tmp_count" "$count_file"
echo "backup written: $dump"

# Prune only after the new backup passes its check, so a bad run never costs
# us older good backups.
if ! scripts/restore-check.sh "$dump"; then
  echo "backup FAILED its restore check; older backups were kept: $dump" >&2
  exit 1
fi

# Names sort chronologically. Keep the newest $KEEP pairs.
shopt -s nullglob
dumps=("$SUMMA_BACKUP_DIR"/summa-*.dump)
if ((${#dumps[@]} > KEEP)); then
  for old in "${dumps[@]:0:${#dumps[@]}-KEEP}"; do
    rm -f "$old" "${old%.dump}.count"
    echo "pruned: $old"
  done
fi

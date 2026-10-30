#!/usr/bin/env bash
# Local backup of the ledger database: a pg_dump snapshot, plus a restore check.
#
#   SUMMA_DB          database to back up        (default: summa)
#   SUMMA_BACKUP_DIR  where dumps are written    (default: $HOME/.summa/backups)
set -euo pipefail

SUMMA_DB="${SUMMA_DB:-summa}"
# restore-check.sh drops and recreates this database, so never back it up.
RESTORE_CHECK_DB=summa_restore_check
if [[ "$SUMMA_DB" == "$RESTORE_CHECK_DB" ]]; then
  echo "refusing to back up $RESTORE_CHECK_DB: the restore check drops and recreates it" >&2
  exit 1
fi
SUMMA_BACKUP_DIR="${SUMMA_BACKUP_DIR:-$HOME/.summa/backups}"
# Make the path absolute relative to the caller's directory, before we cd.
case "$SUMMA_BACKUP_DIR" in
  /*) ;;
  *) SUMMA_BACKUP_DIR="$PWD/$SUMMA_BACKUP_DIR" ;;
esac

cd "$(dirname "${BASH_SOURCE[0]}")/.."
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
  # Rename so the pair no longer matches summa-*.dump: it must not count toward
  # the newest $KEEP or be picked as the "newest" dump. Kept for inspection.
  mv "$dump" "$dump.failed"
  mv "$count_file" "$count_file.failed"
  echo "backup FAILED its restore check; kept for inspection: $dump.failed and $count_file.failed" >&2
  echo "older backups were kept (nothing pruned)" >&2
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

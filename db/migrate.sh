#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MIGRATIONS_DIR="$SCRIPT_DIR/migrations"

if [ -n "${PGHOST:-}" ]; then
    PSQL=(psql)
else
    PSQL=(docker compose exec -T postgres psql -U summa -d summa)
fi

"${PSQL[@]}" -v ON_ERROR_STOP=1 -c "
CREATE TABLE IF NOT EXISTS schema_migrations (
    version    TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"

shopt -s nullglob
for file in "$MIGRATIONS_DIR"/*.sql; do
    version="$(basename "$file")"
    already_applied=$("${PSQL[@]}" -tAc \
        "SELECT 1 FROM schema_migrations WHERE version = '$version';")

    if [ "$already_applied" = "1" ]; then
        echo "skip  $version (already applied)"
        continue
    fi

    echo "apply $version"
    {
        echo "BEGIN;"
        cat "$file"
        echo
        echo "INSERT INTO schema_migrations (version) VALUES ('$version');"
        echo "COMMIT;"
    } | "${PSQL[@]}" -v ON_ERROR_STOP=1
done

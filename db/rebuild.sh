#!/usr/bin/env bash
set -euo pipefail

PSQL=(docker compose exec -T postgres psql -U summa -d summa)

echo "Truncating ledger.account_balances and resetting the checkpoint to 0..."
"${PSQL[@]}" -v ON_ERROR_STOP=1 -c "
BEGIN;
TRUNCATE TABLE ledger.account_balances;
UPDATE ledger.account_balances_checkpoint SET last_seq = 0 WHERE id = 1;
COMMIT;
"
echo "Done. Restart Summa.Ledger.Projections to replay from sequence 0."

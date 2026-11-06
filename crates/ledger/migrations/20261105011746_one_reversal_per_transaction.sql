-- A transaction is reversed at most once. Nulls don't collide, so ordinary
-- transactions (reverses IS NULL) are unaffected. The unique constraint (which
-- has its own index) replaces the plain index.
DROP INDEX transactions_reverses_idx;
ALTER TABLE transactions ADD CONSTRAINT transactions_reverses_unique UNIQUE (reverses);

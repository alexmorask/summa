-- A transaction is reversed at most once. Nulls don't collide, so ordinary
-- transactions (reverses IS NULL) are unaffected. The unique index replaces
-- the plain one.
DROP INDEX transactions_reverses_idx;
ALTER TABLE transactions ADD CONSTRAINT transactions_reverses_unique UNIQUE (reverses);

-- Append-only: rows are never updated or deleted, and tables are never truncated.
CREATE FUNCTION refuse_ledger_edit() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'ledger tables are append-only';
END;
$$;

CREATE TRIGGER ledger_append_only_rows BEFORE UPDATE OR DELETE ON transactions
    FOR EACH ROW EXECUTE FUNCTION refuse_ledger_edit();
CREATE TRIGGER ledger_append_only_truncate BEFORE TRUNCATE ON transactions
    FOR EACH STATEMENT EXECUTE FUNCTION refuse_ledger_edit();

CREATE TRIGGER ledger_append_only_rows BEFORE UPDATE OR DELETE ON entries
    FOR EACH ROW EXECUTE FUNCTION refuse_ledger_edit();
CREATE TRIGGER ledger_append_only_truncate BEFORE TRUNCATE ON entries
    FOR EACH STATEMENT EXECUTE FUNCTION refuse_ledger_edit();

CREATE TRIGGER ledger_append_only_rows BEFORE UPDATE OR DELETE ON entry_tags
    FOR EACH ROW EXECUTE FUNCTION refuse_ledger_edit();
CREATE TRIGGER ledger_append_only_truncate BEFORE TRUNCATE ON entry_tags
    FOR EACH STATEMENT EXECUTE FUNCTION refuse_ledger_edit();

-- Balance backstop: a transaction needs at least two entries, and its debits
-- must equal its credits. The Rust types enforce this too; this catches anything
-- that writes to the tables directly.
CREATE FUNCTION check_transaction_balanced(checked_id bigint) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    entry_count bigint;
    debits      bigint;
    credits     bigint;
BEGIN
    SELECT count(*),
           coalesce(sum(amount) FILTER (WHERE side = 'debit'), 0),
           coalesce(sum(amount) FILTER (WHERE side = 'credit'), 0)
    INTO entry_count, debits, credits
    FROM entries
    WHERE transaction_id = checked_id;

    IF entry_count < 2 THEN
        RAISE EXCEPTION 'transaction % needs at least two entries', checked_id;
    END IF;
    IF debits <> credits THEN
        RAISE EXCEPTION 'transaction % is not balanced: debits %, credits %',
            checked_id, debits, credits;
    END IF;
END;
$$;

CREATE FUNCTION check_transaction_balanced_after_transaction_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM check_transaction_balanced(NEW.id);
    RETURN NULL;
END;
$$;

CREATE FUNCTION check_transaction_balanced_after_entry_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM check_transaction_balanced(NEW.transaction_id);
    RETURN NULL;
END;
$$;

-- Deferred, so the check runs at commit, after every entry has been inserted.
-- The transactions trigger catches a transaction with no entries at all.
CREATE CONSTRAINT TRIGGER transactions_balanced_at_commit AFTER INSERT ON transactions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_transaction_balanced_after_transaction_insert();
CREATE CONSTRAINT TRIGGER entries_balanced_at_commit AFTER INSERT ON entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_transaction_balanced_after_entry_insert();

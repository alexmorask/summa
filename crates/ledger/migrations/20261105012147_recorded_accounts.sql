-- The ledger's own record of each account it has posted to, written on the
-- account's first entry. Accounts are defined outside the ledger; this table
-- keeps only the facts that give past entries their meaning, so they can't
-- change. No name: a name is only a label. Append-only like every ledger table.
CREATE TABLE recorded_accounts (
    number       bigint  NOT NULL,
    account_type text    NOT NULL,
    contra       boolean NOT NULL,
    CONSTRAINT recorded_accounts_pkey PRIMARY KEY (number),
    CONSTRAINT recorded_accounts_number_range CHECK (number BETWEEN 0 AND 4294967295),
    CONSTRAINT recorded_accounts_type_valid
        CHECK (account_type IN ('asset', 'liability', 'equity', 'revenue', 'expense'))
);

CREATE TRIGGER ledger_append_only_rows BEFORE UPDATE OR DELETE ON recorded_accounts
    FOR EACH ROW EXECUTE FUNCTION refuse_ledger_edit();
CREATE TRIGGER ledger_append_only_truncate BEFORE TRUNCATE ON recorded_accounts
    FOR EACH STATEMENT EXECUTE FUNCTION refuse_ledger_edit();

ALTER TABLE entries ADD CONSTRAINT entries_account_fkey
    FOREIGN KEY (account) REFERENCES recorded_accounts (number);

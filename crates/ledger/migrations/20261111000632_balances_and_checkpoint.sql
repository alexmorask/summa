-- Stored balances: running debit and credit totals per account, and per account
-- and tag. They are derived from the entries, so they are not append-only: the
-- worker updates them, and they can be deleted and rebuilt from the log.
CREATE TABLE account_balances (
    account      bigint NOT NULL,
    debit_total  bigint NOT NULL DEFAULT 0,
    credit_total bigint NOT NULL DEFAULT 0,
    CONSTRAINT account_balances_pkey PRIMARY KEY (account),
    CONSTRAINT account_balances_account_fkey FOREIGN KEY (account) REFERENCES recorded_accounts (number),
    CONSTRAINT account_balances_totals_non_negative CHECK (debit_total >= 0 AND credit_total >= 0)
);

CREATE TABLE tag_balances (
    account      bigint NOT NULL,
    kind         text   NOT NULL,
    value        text   NOT NULL,
    debit_total  bigint NOT NULL DEFAULT 0,
    credit_total bigint NOT NULL DEFAULT 0,
    CONSTRAINT tag_balances_pkey PRIMARY KEY (account, kind, value),
    CONSTRAINT tag_balances_account_fkey FOREIGN KEY (account) REFERENCES recorded_accounts (number),
    CONSTRAINT tag_balances_totals_non_negative CHECK (debit_total >= 0 AND credit_total >= 0)
);

-- Where the worker has read up to in the log, in (db_tx_id, id) order. One row,
-- seeded at the start of the log.
CREATE TABLE balance_checkpoint (
    only_row       boolean NOT NULL DEFAULT true,
    db_tx_id       xid8    NOT NULL,
    transaction_id bigint  NOT NULL,
    CONSTRAINT balance_checkpoint_pkey PRIMARY KEY (only_row),
    CONSTRAINT balance_checkpoint_only_row CHECK (only_row)
);

INSERT INTO balance_checkpoint (only_row, db_tx_id, transaction_id) VALUES (true, '0'::xid8, 0);

-- The worker reads new transactions in this order.
CREATE INDEX transactions_log_order_idx ON transactions (db_tx_id, id);

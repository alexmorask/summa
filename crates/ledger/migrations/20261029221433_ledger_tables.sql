-- Ids come from this sequence; the store always supplies them.
CREATE SEQUENCE transaction_ids;

CREATE TABLE transactions (
    id              bigint      NOT NULL,
    idempotency_key text        NOT NULL,
    description     text        NOT NULL,
    effective_at    timestamptz NOT NULL,
    -- Always supplied by the application's clock, never by the database.
    recorded_at     timestamptz NOT NULL,
    reverses        bigint      NULL,
    -- The database transaction that wrote the row; used to order commits.
    db_tx_id        xid8        NOT NULL DEFAULT pg_current_xact_id(),
    CONSTRAINT transactions_pkey PRIMARY KEY (id),
    CONSTRAINT transactions_idempotency_key_unique UNIQUE (idempotency_key),
    CONSTRAINT transactions_reverses_fkey FOREIGN KEY (reverses) REFERENCES transactions (id),
    CONSTRAINT transactions_effective_not_after_recorded CHECK (effective_at <= recorded_at)
);

CREATE INDEX transactions_reverses_idx ON transactions (reverses);

CREATE TABLE entries (
    transaction_id bigint   NOT NULL,
    position       smallint NOT NULL,
    account        bigint   NOT NULL,
    side           text     NOT NULL,
    amount         bigint   NOT NULL,
    currency       text     NOT NULL,
    CONSTRAINT entries_pkey PRIMARY KEY (transaction_id, position),
    CONSTRAINT entries_transaction_fkey FOREIGN KEY (transaction_id) REFERENCES transactions (id),
    CONSTRAINT entries_position_non_negative CHECK (position >= 0),
    CONSTRAINT entries_account_range CHECK (account BETWEEN 0 AND 4294967295),
    CONSTRAINT entries_side_valid CHECK (side IN ('debit', 'credit')),
    CONSTRAINT entries_amount_positive CHECK (amount > 0),
    CONSTRAINT entries_currency_usd CHECK (currency = 'USD')
);

CREATE TABLE entry_tags (
    transaction_id bigint   NOT NULL,
    position       smallint NOT NULL,
    kind           text     NOT NULL,
    value          text     NOT NULL,
    CONSTRAINT entry_tags_pkey PRIMARY KEY (transaction_id, position, kind),
    CONSTRAINT entry_tags_entry_fkey FOREIGN KEY (transaction_id, position) REFERENCES entries (transaction_id, position)
);

CREATE TABLE ledger.events (
    seq             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    transaction_id  UUID NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL,
    correlation_id  TEXT NOT NULL,
    causation_id    TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    payload         JSONB NOT NULL,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

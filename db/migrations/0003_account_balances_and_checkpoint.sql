CREATE TABLE ledger.account_balances (
    account_id TEXT NOT NULL PRIMARY KEY,
    balance    BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE ledger.account_balances_checkpoint (
    id       INT NOT NULL PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    last_seq BIGINT NOT NULL DEFAULT 0
);

INSERT INTO ledger.account_balances_checkpoint (last_seq) VALUES (0);

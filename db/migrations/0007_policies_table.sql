CREATE TABLE policy.policies (
    policy_id       UUID NOT NULL PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    pricing         JSONB NOT NULL,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE FUNCTION policy.reject_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'policy.policies is immutable once published (% rejected)', TG_OP;
END;
$$;

CREATE TRIGGER policies_no_update_delete
    BEFORE UPDATE OR DELETE ON policy.policies
    FOR EACH ROW EXECUTE FUNCTION policy.reject_mutation();

CREATE TRIGGER policies_no_truncate
    BEFORE TRUNCATE ON policy.policies
    FOR EACH STATEMENT EXECUTE FUNCTION policy.reject_mutation();

ALTER TABLE policy.policies ENABLE ALWAYS TRIGGER policies_no_update_delete;
ALTER TABLE policy.policies ENABLE ALWAYS TRIGGER policies_no_truncate;

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS indexer_state (
    key TEXT PRIMARY KEY,
    last_checkpoint_seq BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS chain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tx_digest TEXT NOT NULL,
    event_index INTEGER NOT NULL,
    checkpoint_seq BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tx_digest, event_index)
);

CREATE INDEX IF NOT EXISTS idx_chain_events_checkpoint ON chain_events (checkpoint_seq DESC);

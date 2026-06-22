CREATE TABLE IF NOT EXISTS search_history (
    search_history_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    query TEXT NOT NULL,
    query_key TEXT GENERATED ALWAYS AS (lower(btrim(query))) STORED,
    filter_types TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT search_history_query_not_blank CHECK (char_length(btrim(query)) > 0),
    CONSTRAINT search_history_user_query_key UNIQUE (user_id, query_key)
);

CREATE INDEX IF NOT EXISTS idx_search_history_user_updated
    ON search_history (user_id, updated_at DESC);

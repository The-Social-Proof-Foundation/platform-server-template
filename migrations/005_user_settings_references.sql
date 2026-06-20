CREATE TABLE IF NOT EXISTS user_references (
    reference_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    reference_type TEXT NOT NULL,
    reference_key TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, reference_type, reference_key)
);

CREATE INDEX IF NOT EXISTS idx_user_references_user_type
    ON user_references (user_id, reference_type, created_at DESC);

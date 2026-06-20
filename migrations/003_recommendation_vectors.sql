CREATE TABLE IF NOT EXISTS user_vectors (
    wallet_address TEXT PRIMARY KEY,
    profile_embedding vector(3072),
    interest_categories JSONB,
    engagement_patterns JSONB,
    social_signals JSONB,
    embedding_model TEXT,
    embedding_dim INT,
    last_updated TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS content_vectors (
    content_id TEXT PRIMARY KEY,
    creator_wallet_address TEXT,
    platform_id TEXT,
    content_embedding vector(3072),
    category_tags TEXT[],
    hashtags TEXT[],
    description TEXT,
    mentions TEXT[],
    audio_transcribe TEXT,
    duration INTEGER,
    source_timestamp TIMESTAMPTZ,
    extra_metadata JSONB,
    audio_features JSONB,
    performance_metrics JSONB,
    embedding_model TEXT,
    embedding_dim INT,
    nsfw BOOLEAN NOT NULL DEFAULT FALSE,
    moderation JSONB,
    moderation_override TEXT CHECK (moderation_override IN ('force_allow', 'force_block')),
    moderation_reviewed_by TEXT,
    moderation_reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_address TEXT NOT NULL,
    content_id TEXT NOT NULL,
    interaction_type VARCHAR(20) NOT NULL,
    engagement_score FLOAT,
    watch_duration INTEGER,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    context_data JSONB
);

CREATE INDEX IF NOT EXISTS idx_content_vectors_created ON content_vectors (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_content_vectors_nsfw_created ON content_vectors (nsfw, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_content_vectors_platform_created ON content_vectors (platform_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_content_vectors_nsfw_false_created
    ON content_vectors (created_at DESC) WHERE nsfw = FALSE;

CREATE INDEX IF NOT EXISTS idx_user_interactions_wallet_time
    ON user_interactions (wallet_address, timestamp DESC);

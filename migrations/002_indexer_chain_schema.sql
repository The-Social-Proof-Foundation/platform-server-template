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

ALTER TABLE posts ADD COLUMN IF NOT EXISTS chain_post_id TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS platform_id TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS owner_address TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS tx_digest TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS checkpoint_seq BIGINT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS post_type TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS media_urls JSONB;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS metadata_json JSONB;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_posts_chain_post_id ON posts (chain_post_id) WHERE chain_post_id IS NOT NULL;

ALTER TABLE comments ADD COLUMN IF NOT EXISTS chain_comment_id TEXT;
ALTER TABLE comments ADD COLUMN IF NOT EXISTS tx_digest TEXT;
ALTER TABLE comments ADD COLUMN IF NOT EXISTS checkpoint_seq BIGINT;
ALTER TABLE comments ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_comments_chain_comment_id ON comments (chain_comment_id) WHERE chain_comment_id IS NOT NULL;

ALTER TABLE likes ADD COLUMN IF NOT EXISTS tx_digest TEXT;
ALTER TABLE likes ADD COLUMN IF NOT EXISTS reaction TEXT;
ALTER TABLE likes ADD COLUMN IF NOT EXISTS checkpoint_seq BIGINT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_likes_liker_target_reaction
    ON likes (liker_wallet_address, target_id, target_type, COALESCE(reaction, ''));

ALTER TABLE users ADD COLUMN IF NOT EXISTS chain_address TEXT;
UPDATE users
SET chain_address = LOWER(wallet_address)
WHERE chain_address IS NULL AND wallet_address IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_chain_address ON users (chain_address) WHERE chain_address IS NOT NULL;

CREATE TABLE IF NOT EXISTS tips (
    tip_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_id TEXT NOT NULL,
    from_wallet_address TEXT,
    to_wallet_address TEXT,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount BIGINT NOT NULL,
    coin_type TEXT,
    is_post BOOLEAN NOT NULL DEFAULT TRUE,
    tx_digest TEXT NOT NULL,
    checkpoint_seq BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tips_to_address ON tips (to_address, created_at DESC);

CREATE TABLE IF NOT EXISTS chain_post_map (
    chain_post_id TEXT PRIMARY KEY,
    post_id UUID NOT NULL REFERENCES posts (post_id) ON DELETE CASCADE
);

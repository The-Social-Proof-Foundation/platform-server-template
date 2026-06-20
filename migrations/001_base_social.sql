-- Base social schema (wallet-first)

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_address TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL UNIQUE,
    chain_address TEXT UNIQUE,
    username TEXT,
    full_name TEXT,
    bio TEXT,
    email TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    profile_image TEXT,
    profile_image_icon TEXT,
    cover_image TEXT,
    follower_count INTEGER NOT NULL DEFAULT 0,
    following_count INTEGER NOT NULL DEFAULT 0,
    notification_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users (LOWER(username));

CREATE TABLE IF NOT EXISTS posts (
    post_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_wallet_address TEXT NOT NULL,
    description TEXT,
    hashtags TEXT[] NOT NULL DEFAULT '{}',
    mentions TEXT[] NOT NULL DEFAULT '{}',
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_posts_author ON posts (author_wallet_address, timestamp DESC);

CREATE TABLE IF NOT EXISTS comments (
    comment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id UUID NOT NULL REFERENCES posts (post_id) ON DELETE CASCADE,
    commenter_wallet_address TEXT NOT NULL,
    content TEXT,
    hashtags TEXT[] NOT NULL DEFAULT '{}',
    mentions TEXT[] NOT NULL DEFAULT '{}',
    timestamp BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE TABLE IF NOT EXISTS likes (
    like_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    liker_wallet_address TEXT NOT NULL REFERENCES users (wallet_address),
    target_id UUID NOT NULL,
    target_type TEXT NOT NULL,
    reaction TEXT,
    tx_digest TEXT,
    checkpoint_seq BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS follows (
    follower_wallet_address TEXT NOT NULL REFERENCES users (wallet_address),
    followee_wallet_address TEXT NOT NULL REFERENCES users (wallet_address),
    followed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (follower_wallet_address, followee_wallet_address)
);

CREATE TABLE IF NOT EXISTS blocked (
    blocker_wallet_address TEXT NOT NULL REFERENCES users (wallet_address),
    blocked_wallet_address TEXT NOT NULL REFERENCES users (wallet_address),
    blocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (blocker_wallet_address, blocked_wallet_address)
);

CREATE TABLE IF NOT EXISTS settings (
    user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    setting_name TEXT NOT NULL,
    setting_value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, setting_name)
);

CREATE TABLE IF NOT EXISTS notifications (
    notification_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    sender_id UUID,
    sender_wallet_address TEXT,
    type TEXT NOT NULL,
    object_id TEXT,
    object_type TEXT,
    title TEXT,
    message TEXT,
    image_1 TEXT,
    image_2 TEXT,
    read_at TIMESTAMPTZ,
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS device_tokens (
    user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    device_token TEXT NOT NULL,
    device_type TEXT NOT NULL DEFAULT 'ios',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, device_token)
);

CREATE TABLE IF NOT EXISTS post_views (
    view_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id UUID NOT NULL REFERENCES posts (post_id) ON DELETE CASCADE,
    viewer_wallet_address TEXT NOT NULL,
    watch_duration INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS platforms (
    platform_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

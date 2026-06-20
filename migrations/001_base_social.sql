-- Base platform schema (wallet-first). Social reads use MySocial GraphQL. No local mirror tables.

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
    notification_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users (LOWER(username));
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_chain_address ON users (chain_address) WHERE chain_address IS NOT NULL;

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

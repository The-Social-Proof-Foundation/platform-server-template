CREATE TABLE IF NOT EXISTS platform_delivery_config (
    id BIGSERIAL PRIMARY KEY,
    platform_id TEXT NOT NULL UNIQUE,
    apns_bundle_id TEXT,
    apns_key_id TEXT,
    apns_team_id TEXT,
    apns_key_path TEXT,
    apns_key_content TEXT,
    fcm_server_key TEXT,
    resend_api_key TEXT,
    resend_from_email TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_platform_delivery_config_platform_id
    ON platform_delivery_config (platform_id);

CREATE TABLE IF NOT EXISTS analytics_outbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    topic TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    idempotency_key TEXT UNIQUE,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_analytics_outbox_unpublished
    ON analytics_outbox (created_at ASC) WHERE published_at IS NULL;

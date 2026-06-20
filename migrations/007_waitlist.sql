-- Waitlist and early access

ALTER TABLE users ADD COLUMN IF NOT EXISTS referral_code TEXT UNIQUE;

CREATE INDEX IF NOT EXISTS idx_users_referral_code ON users (referral_code)
    WHERE referral_code IS NOT NULL;

CREATE SEQUENCE IF NOT EXISTS waitlist_queue_seq;

CREATE TABLE IF NOT EXISTS waitlist_config (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    admission_interval_hours INTEGER NOT NULL DEFAULT 24 CHECK (admission_interval_hours IN (12, 24)),
    spots_per_batch INTEGER NOT NULL DEFAULT 100 CHECK (spots_per_batch > 0),
    is_paused BOOLEAN NOT NULL DEFAULT false,
    last_batch_at TIMESTAMPTZ,
    next_batch_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO waitlist_config (id, admission_interval_hours, spots_per_batch, next_batch_at)
VALUES (1, 24, 100, NOW())
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS waitlist_entries (
    user_id UUID PRIMARY KEY REFERENCES users (user_id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'waiting' CHECK (status IN ('waiting', 'approved')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    queue_score BIGINT NOT NULL,
    referral_bumps INTEGER NOT NULL DEFAULT 0,
    approved_at TIMESTAMPTZ,
    approved_via TEXT CHECK (approved_via IS NULL OR approved_via IN ('batch', 'invite', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_waitlist_entries_queue
    ON waitlist_entries (status, queue_score ASC);

CREATE TABLE IF NOT EXISTS waitlist_user_controls (
    user_id UUID PRIMARY KEY REFERENCES users (user_id) ON DELETE CASCADE,
    invites_enabled BOOLEAN NOT NULL DEFAULT false,
    manual_priority_boost INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

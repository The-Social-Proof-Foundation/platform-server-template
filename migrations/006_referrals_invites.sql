CREATE TABLE IF NOT EXISTS user_referrals (
    referral_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    referrer_user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    referred_user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    referral_code TEXT,
    status TEXT NOT NULL DEFAULT 'completed',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (referred_user_id)
);

CREATE INDEX IF NOT EXISTS idx_user_referrals_referrer
    ON user_referrals (referrer_user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS user_invites (
    invite_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    inviter_user_id UUID NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    invite_code TEXT NOT NULL UNIQUE,
    invitee_user_id UUID REFERENCES users (user_id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_user_invites_inviter
    ON user_invites (inviter_user_id, created_at DESC);

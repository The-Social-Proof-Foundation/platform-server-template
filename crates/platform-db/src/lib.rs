pub mod counters;
pub mod invite;
pub mod migrations;
pub mod outbox;
pub mod redis_store;
pub mod referral;
pub mod settings;
pub mod user_references;
pub mod waitlist;

pub use counters::{CounterFlushManager, ShardedCounter};
pub use invite::{
    accept_invite, count_active_invites, count_circulating_invites, create_invite, get_invite_by_code,
    list_invites, on_invite_accepted, InviteCirculationStats, InviteRow, INVITE_EXPIRY_DAYS,
    MAX_ACCEPTED_INVITES_PER_USER, MAX_INVITES_PER_USER,
};
pub use migrations::{default_migrations_dir, run_migrations};
pub use outbox::{fetch_unpublished_outbox, insert_outbox_event, mark_outbox_published, OutboxRow};
pub use redis_store::{
    consume_auth_nonce, delete_refresh_token, get_refresh_token, is_user_online, rate_limit_incr,
    set_user_online, store_auth_nonce, store_refresh_token,
};
pub use referral::{
    count_completed_referrals, list_referrals, on_referral_threshold_reached, record_referral,
    referral_stats, resolve_referrer_by_code, ReferralRow, REFERRAL_MIN_ACCOUNT_AGE_DAYS,
    REFERRALS_REQUIRED,
};
pub use settings::{
    blocked_count, delete_setting, get_bool_setting, get_setting, list_settings, upsert_setting,
    UserSettingRow,
};
pub use user_references::{
    delete_reference, exists_reference, list_references, upsert_reference, ReferenceInput,
    UserReferenceRow,
};
pub use waitlist::{
    admin_stats, apply_referral_bump, approve_user, assign_referral_code, can_user_create_invites,
    generate_referral_code, get_config, get_referral_code, get_status, is_approved, join_waitlist,
    normalize_wallet_lookup, resolve_waitlist_user, resume_batches, run_admission_batch,
    select_next_batch, set_config, set_paused, set_user_invites_enabled, user_invites_enabled,
    validate_mint_invites_count, waitlist_entry_status, BatchAdmissionResult, WaitlistAdminStats,
    WaitlistConfigRow, WaitlistStatus, WaitlistUserSummary, DEFAULT_ADMISSION_INTERVAL_HOURS,
    DEFAULT_SPOTS_PER_BATCH, POSITION_ESTIMATE_BATCHES, REFERRAL_BUMP_POINTS,
};

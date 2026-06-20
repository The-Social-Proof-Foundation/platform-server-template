use crate::state::SharedApiState;

pub async fn dispatch_referral_signup_notifications(
    state: &SharedApiState,
    referrer_user_id: &str,
    referred_user_id: &str,
    bump_applied: bool,
) {
    let mut redis = state.redis();
    if bump_applied {
        let _ = state
            .notify
            .notify_waitlist_bump(state.pg(), &mut redis, referrer_user_id)
            .await
            .inspect_err(|e| tracing::warn!("waitlist_bump notification failed: {e}"));
    }
    let _ = state
        .notify
        .notify_referral_claimed(state.pg(), &mut redis, referrer_user_id, referred_user_id)
        .await
        .inspect_err(|e| tracing::warn!("referral_claimed notification failed: {e}"));
}

pub async fn dispatch_waitlist_joined(state: &SharedApiState, user_id: &str) {
    let mut redis = state.redis();
    let _ = state
        .notify
        .notify_waitlist_joined(state.pg(), &mut redis, user_id)
        .await
        .inspect_err(|e| tracing::warn!("waitlist_joined notification failed: {e}"));
}

pub async fn dispatch_waitlist_approved(
    state: &SharedApiState,
    user_id: &str,
    invites_minted: u32,
) {
    let mut redis = state.redis();
    let _ = state
        .notify
        .notify_waitlist_approved(state.pg(), &mut redis, user_id, invites_minted)
        .await
        .inspect_err(|e| tracing::warn!("waitlist_approved notification failed: {e}"));
}

pub async fn dispatch_invite_accepted(
    state: &SharedApiState,
    inviter_user_id: &str,
    invitee_user_id: &str,
) {
    let mut redis = state.redis();
    let _ = state
        .notify
        .notify_invite_accepted(state.pg(), &mut redis, inviter_user_id, invitee_user_id)
        .await
        .inspect_err(|e| tracing::warn!("invite_accepted notification failed: {e}"));
}

pub async fn dispatch_batch_admissions(state: &SharedApiState, user_ids: &[String]) {
    for user_id in user_ids {
        dispatch_waitlist_approved(state, user_id, 0).await;
    }
}

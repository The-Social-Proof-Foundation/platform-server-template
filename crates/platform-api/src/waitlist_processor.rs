use std::time::Duration;

use platform_db::run_admission_batch;
use tracing::{info, warn};

use crate::state::SharedApiState;
use crate::waitlist_events::dispatch_batch_admissions;

pub fn spawn_waitlist_processor(state: SharedApiState) {
    if !state.config().waitlist_enabled {
        return;
    }

    let batch_enabled = state.config().waitlist_batch_admission_enabled;
    tokio::spawn(async move {
        info!("waitlist batch processor started (batch_admission_enabled={batch_enabled})");
        loop {
            if state.config().waitlist_enabled {
                let enabled = state.config().effective_batch_admission();
                match run_admission_batch(state.pg(), enabled).await {
                    Ok(result) => {
                        if !result.admitted_user_ids.is_empty() {
                            info!(
                                count = result.admitted_user_ids.len(),
                                "waitlist batch admitted users"
                            );
                            dispatch_batch_admissions(&state, &result.admitted_user_ids).await;
                        }
                    }
                    Err(err) => warn!(error = %err, "waitlist batch processor error"),
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

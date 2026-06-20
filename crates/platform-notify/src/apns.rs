use std::fs::File;
use std::io::Read;

use a2::{
    Client, ClientConfig, DefaultNotificationBuilder, Endpoint, NotificationBuilder,
    NotificationOptions, Priority, PushType,
};
use platform_core::{AppError, AppResult, Config};
use tracing::warn;

#[derive(Clone)]
pub struct ApnsClient {
    inner: Option<Client>,
    bundle_id: String,
}

impl ApnsClient {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let bundle_id = config
            .apns_bundle_id
            .clone()
            .unwrap_or_else(|| "com.projectyz.app".into());

        let inner = match (
            config.apns_key_id.as_deref(),
            config.apns_team_id.as_deref(),
            config.apns_key_path.as_deref(),
        ) {
            (Some(key_id), Some(team_id), Some(path)) => {
                let mut file = File::open(path)
                    .map_err(|e| AppError::Config(format!("APNs key file not found: {e}")))?;
                let mut key_bytes = Vec::new();
                file.read_to_end(&mut key_bytes)
                    .map_err(|e| AppError::Config(format!("APNs key read failed: {e}")))?;

                let endpoint = match config.apns_environment.to_ascii_lowercase().as_str() {
                    "production" => Endpoint::Production,
                    _ => Endpoint::Sandbox,
                };

                match Client::token(
                    key_bytes.as_slice(),
                    key_id,
                    team_id,
                    ClientConfig::new(endpoint),
                ) {
                    Ok(client) => Some(client),
                    Err(err) => {
                        warn!(error = %err, "failed to initialize APNs client");
                        None
                    }
                }
            }
            _ => None,
        };

        Ok(Self { inner, bundle_id })
    }

    pub async fn send_push(
        &self,
        device_token: &str,
        title: &str,
        body: &str,
        deep_link: Option<String>,
    ) -> AppResult<()> {
        let Some(client) = &self.inner else {
            return Ok(());
        };

        let mut payload = DefaultNotificationBuilder::new()
            .set_title(title)
            .set_body(body)
            .set_sound("default")
            .build(
                device_token,
                NotificationOptions {
                    apns_topic: Some(&self.bundle_id),
                    apns_push_type: Some(PushType::Alert),
                    apns_priority: Some(Priority::High),
                    ..Default::default()
                },
            );

        if let Some(link) = deep_link {
            let _ = payload.add_custom_data("deepLink", &link);
        }

        client
            .send(payload)
            .await
            .map_err(|e| AppError::Internal(format!("APNs send failed: {e}")))?;
        Ok(())
    }
}

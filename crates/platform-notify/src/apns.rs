use std::fs::File;
use std::io::Read;

use a2::{
    Client, ClientConfig, DefaultNotificationBuilder, Endpoint, NotificationBuilder,
    NotificationOptions, Priority, PushType,
};
use platform_core::{AppError, AppResult, Config};
use platform_db::DeliveryConfigRow;
use tracing::warn;

#[derive(Clone)]
pub struct ApnsClient {
    inner: Option<Client>,
    bundle_id: String,
}

impl ApnsClient {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        Self::build(
            config.apns_bundle_id.as_deref(),
            config.apns_key_id.as_deref(),
            config.apns_team_id.as_deref(),
            config.apns_key_path.as_deref(),
            None,
            &config.apns_environment,
        )
    }

    pub fn from_delivery(global: &Config, delivery: Option<&DeliveryConfigRow>) -> AppResult<Self> {
        let bundle_id = delivery
            .and_then(|d| d.apns_bundle_id.as_deref())
            .or(global.apns_bundle_id.as_deref());
        let key_id = delivery
            .and_then(|d| d.apns_key_id.as_deref())
            .or(global.apns_key_id.as_deref());
        let team_id = delivery
            .and_then(|d| d.apns_team_id.as_deref())
            .or(global.apns_team_id.as_deref());
        let key_path = delivery
            .and_then(|d| d.apns_key_path.as_deref())
            .or(global.apns_key_path.as_deref());
        let key_content = delivery.and_then(|d| d.apns_key_content.as_deref());
        Self::build(bundle_id, key_id, team_id, key_path, key_content, &global.apns_environment)
    }

    fn build(
        bundle_id: Option<&str>,
        key_id: Option<&str>,
        team_id: Option<&str>,
        key_path: Option<&str>,
        key_content: Option<&str>,
        environment: &str,
    ) -> AppResult<Self> {
        let bundle_id = bundle_id.unwrap_or("com.projectyz.app").to_string();

        let key_bytes = if let Some(content) = key_content {
            Some(content.as_bytes().to_vec())
        } else if let Some(path) = key_path {
            let mut file = File::open(path)
                .map_err(|e| AppError::Config(format!("APNs key file not found: {e}")))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| AppError::Config(format!("APNs key read failed: {e}")))?;
            Some(bytes)
        } else {
            None
        };

        let inner = match (key_id, team_id, key_bytes) {
            (Some(key_id), Some(team_id), Some(key_bytes)) => {
                let endpoint = match environment.to_ascii_lowercase().as_str() {
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

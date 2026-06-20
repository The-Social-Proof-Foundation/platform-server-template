use platform_core::{AppError, AppResult};
use reqwest::Client;
use serde_json::json;
use tracing::warn;

#[derive(Clone)]
pub struct FcmClient {
    http: Client,
    server_key: Option<String>,
}

impl FcmClient {
    pub fn new(server_key: Option<String>) -> Self {
        Self {
            http: Client::new(),
            server_key,
        }
    }

    pub async fn send_push(
        &self,
        token: &str,
        title: &str,
        body: &str,
        data: Option<serde_json::Value>,
    ) -> AppResult<()> {
        let Some(server_key) = &self.server_key else {
            return Ok(());
        };

        let mut payload = json!({
            "to": token,
            "notification": {
                "title": title,
                "body": body,
            },
        });
        if let Some(data) = data {
            payload["data"] = data;
        }

        let response = self
            .http
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={server_key}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            let body_text = response.text().await.unwrap_or_default();
            warn!(body = body_text, "FCM delivery failed");
            return Err(AppError::Internal(format!("FCM error: {body_text}")));
        }
        Ok(())
    }
}

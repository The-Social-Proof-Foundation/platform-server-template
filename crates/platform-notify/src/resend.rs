use platform_core::{AppError, AppResult, Config};
use reqwest::Client;
use serde_json::json;
use tracing::warn;

#[derive(Clone)]
pub struct ResendClient {
    http: Client,
    api_key: Option<String>,
    from_email: Option<String>,
}

impl ResendClient {
    pub fn from_config(config: &Config) -> Self {
        Self {
            http: Client::new(),
            api_key: config.resend_api_key.clone(),
            from_email: config.resend_from_email.clone(),
        }
    }

    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        html: &str,
        api_key: Option<&str>,
        from_email: Option<&str>,
    ) -> AppResult<()> {
        let api_key = api_key
            .or(self.api_key.as_deref())
            .ok_or_else(|| AppError::Config("Resend API key not configured".into()))?;
        let from = from_email
            .or(self.from_email.as_deref())
            .ok_or_else(|| AppError::Config("Resend from email not configured".into()))?;

        let response = self
            .http
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&json!({
                "from": from,
                "to": [to],
                "subject": subject,
                "html": html,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            warn!(body, "resend delivery failed");
            return Err(AppError::Internal(format!("Resend error: {body}")));
        }
        Ok(())
    }
}

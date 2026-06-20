use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use platform_core::AppError;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError(pub AppError);

impl<T> From<T> for ApiError
where
    AppError: From<T>,
{
    fn from(value: T) -> Self {
        Self(AppError::from(value))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = self.0;
        let status =
            StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "error": err.to_string() }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

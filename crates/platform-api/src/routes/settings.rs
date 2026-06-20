use axum::extract::{Extension, Query};
use axum::Json;
use platform_core::AppError;
use platform_db::{
    blocked_count, delete_setting, list_references, list_settings, upsert_reference,
    upsert_setting, UserSettingRow,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::ApiResult;
use crate::middleware::AuthUser;
use crate::state::SharedApiState;

#[derive(Debug, Serialize)]
struct SettingItem {
    setting_name: String,
    setting_value: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    settings: Vec<SettingItem>,
    #[serde(rename = "blockedCount")]
    blocked_count: i64,
}

fn parse_setting_name(body: &Value) -> Result<String, AppError> {
    body.get("settingName")
        .or_else(|| body.get("setting_name"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("settingName required".into()))
}

fn parse_setting_value(body: &Value) -> Result<String, AppError> {
    body.get("settingValue")
        .or_else(|| body.get("setting_value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("settingValue required".into()))
}

pub async fn get_settings(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
) -> ApiResult<Json<SettingsResponse>> {
    let rows = list_settings(state.pg_read(), &auth.user_id).await?;
    let blocked = blocked_count(state.pg_read(), &auth.user_id).await?;
    Ok(Json(SettingsResponse {
        settings: rows
            .into_iter()
            .map(|r: UserSettingRow| SettingItem {
                setting_name: r.setting_name,
                setting_value: r.setting_value,
            })
            .collect(),
        blocked_count: blocked,
    }))
}

pub async fn get_settings_catalog(
    Extension(_state): Extension<SharedApiState>,
) -> ApiResult<Json<Value>> {
    let definitions: Vec<Value> = platform_core::settings::SETTING_DEFINITIONS
        .iter()
        .map(|d| {
            json!({
                "key": d.key,
                "defaultValue": d.default_value,
                "description": d.description,
            })
        })
        .collect();
    Ok(Json(json!({ "definitions": definitions })))
}

pub async fn upsert_setting_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let name = parse_setting_name(&body)?;
    let value = parse_setting_value(&body)?;
    upsert_setting(state.pg(), &auth.user_id, &name, &value).await?;
    Ok(Json(json!({ "message": "Setting added or updated successfully" })))
}

pub async fn delete_setting_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<Value>,
) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let name = parse_setting_name(&body)?;
    let deleted = delete_setting(state.pg(), &auth.user_id, &name).await?;
    if !deleted {
        return Err(AppError::NotFound.into());
    }
    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({ "message": "Setting deleted successfully" })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ReferencesQuery {
    #[serde(rename = "type")]
    pub reference_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn parse_reference_type(body: &Value) -> Result<String, AppError> {
    body.get("referenceType")
        .or_else(|| body.get("reference_type"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("referenceType required".into()))
}

fn parse_reference_key(body: &Value) -> Result<String, AppError> {
    body.get("referenceKey")
        .or_else(|| body.get("reference_key"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("referenceKey required".into()))
}

pub async fn get_references(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<ReferencesQuery>,
) -> ApiResult<Json<Value>> {
    let rows = list_references(
        state.pg_read(),
        &auth.user_id,
        query.reference_type.as_deref(),
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(json!({ "references": rows })))
}

pub async fn upsert_reference_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let reference_type = parse_reference_type(&body)?;
    let reference_key = parse_reference_key(&body)?;
    let metadata = body
        .get("metadata")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let row = upsert_reference(
        state.pg(),
        &auth.user_id,
        &reference_type,
        &reference_key,
        metadata,
    )
    .await?;
    Ok(Json(json!({ "reference": row })))
}

pub async fn delete_reference_handler(
    Extension(state): Extension<SharedApiState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    use platform_db::delete_reference;
    let reference_type = parse_reference_type(&body)?;
    let reference_key = parse_reference_key(&body)?;
    let deleted = delete_reference(
        state.pg(),
        &auth.user_id,
        &reference_type,
        &reference_key,
    )
    .await?;
    if !deleted {
        return Err(AppError::NotFound.into());
    }
    Ok(Json(json!({ "ok": true })))
}

//! 现场照片上传：multipart/form-data，字段 file=<图片>、phase=before|after、可选 label。
//! 文件落盘到本地 uploads 目录，DB 只保存相对路径，通过 /uploads 静态访问。

use axum::{
    extract::{Multipart, Path, State},
    Json,
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{get_order, list_photos};
use crate::state::AppState;

const ALLOWED_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

pub async fn upload(
    State(st): State<AppState>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status == "accepted" || order.status == "completed" {
        return Err(ApiError::conflict("订单已验收，不能再上传照片"));
    }

    let mut phase: Option<String> = None;
    let mut label = String::new();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type = "image/jpeg".to_string();
    let mut ext = "jpg";

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("multipart 解析失败: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "phase" => {
                phase = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            "label" => {
                label = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
            }
            "file" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                if !ALLOWED_TYPES.contains(&ct.as_str()) {
                    return Err(ApiError::bad_request(format!(
                        "仅支持 jpeg/png/webp 图片，收到 {ct}"
                    )));
                }
                content_type = ct.clone();
                ext = match ct.as_str() {
                    "image/png" => "png",
                    "image/webp" => "webp",
                    _ => "jpg",
                };
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
                if bytes.is_empty() {
                    return Err(ApiError::bad_request("图片内容为空"));
                }
                if bytes.len() > 10 * 1024 * 1024 {
                    return Err(ApiError::bad_request("图片不能超过 10MB"));
                }
                file_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let phase = phase.ok_or_else(|| ApiError::bad_request("缺少 phase 字段（before/after）"))?;
    if phase != "before" && phase != "after" {
        return Err(ApiError::bad_request("phase 只能是 before 或 after"));
    }
    let bytes = file_bytes.ok_or_else(|| ApiError::bad_request("缺少 file 图片字段"))?;

    let photo_id = Uuid::new_v4().simple().to_string();
    let filename = format!("{id}_{phase}_{photo_id}.{ext}");
    let rel_path = format!("{id}/{filename}");
    let abs_dir = st.uploads_dir.join(&id);
    tokio::fs::create_dir_all(&abs_dir)
        .await
        .map_err(ApiError::from)?;
    tokio::fs::write(abs_dir.join(&filename), &bytes)
        .await
        .map_err(ApiError::from)?;

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO photos (id, order_id, phase, label, rel_path, content_type, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )
    .bind(&photo_id)
    .bind(&id)
    .bind(&phase)
    .bind(&label)
    .bind(&rel_path)
    .bind(&content_type)
    .bind(&now)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({
        "ok": true,
        "photo_id": photo_id,
        "phase": phase,
        "url": format!("/uploads/{rel_path}"),
    })))
}

pub async fn list(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = get_order(&st.pool, &id).await?;
    let photos = list_photos(&st.pool, &id).await?;
    Ok(Json(json!({ "photos": photos })))
}

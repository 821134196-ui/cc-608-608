//! 零件逐件登记：拆下登记一次，复装回填一次。
//! 已拆下但未复装的零件 = 缺件，缺件会阻止完工与验收。

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{get_order, list_parts};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct AddPartReq {
    pub name: String,
    #[serde(default)]
    pub note: String,
}

pub async fn add_part(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AddPartReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status != "in_progress" {
        return Err(ApiError::conflict(
            "只有施工中的订单才能登记零件（请先到场确认范围）",
        ));
    }
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("零件名称不能为空"));
    }

    // 同一零件不能重复登记（DB 唯一索引兜底），避免“拆一个记两个”造成假性缺件
    let exists = list_parts(&st.pool, &id)
        .await?
        .iter()
        .any(|p| p.name == name);
    if exists {
        return Err(ApiError::conflict(format!("零件「{name}」已登记，请勿重复登记")));
    }

    let part_id = Uuid::new_v4().simple().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO parts (id, order_id, name, note, removed_at)
         VALUES (?1,?2,?3,?4,?5)",
    )
    .bind(&part_id)
    .bind(&id)
    .bind(name)
    .bind(&req.note)
    .bind(&now)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({ "ok": true, "part_id": part_id, "reinstalled": false })))
}

pub async fn list(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = get_order(&st.pool, &id).await?;
    let parts = list_parts(&st.pool, &id).await?;
    let missing: Vec<&str> = parts
        .iter()
        .filter(|p| p.reinstalled_at.is_none())
        .map(|p| p.name.as_str())
        .collect();
    Ok(Json(json!({
        "parts": parts,
        "total": parts.len(),
        "reinstalled": parts.len() - missing.len(),
        "missing": missing,
    })))
}

/// 复装登记：幂等——已经复装的零件再次提交直接返回当前状态，不产生第二条记录。
pub async fn reinstall(
    State(st): State<AppState>,
    Path((id, part_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status == "accepted" || order.status == "completed" {
        return Err(ApiError::conflict("订单已验收，不能再变更复装记录"));
    }

    let part = sqlx::query_as::<_, crate::models::PartRow>(
        "SELECT * FROM parts WHERE id = ?1 AND order_id = ?2",
    )
    .bind(&part_id)
    .bind(&id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("零件登记记录不存在"))?;

    if part.reinstalled_at.is_some() {
        // 幂等返回：重复点击“已复装”不报错也不多记
        return Ok(Json(json!({
            "ok": true, "part_id": part_id, "reinstalled": true, "idempotent": true,
        })));
    }

    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE parts SET reinstalled_at=?1 WHERE id=?2 AND reinstalled_at IS NULL")
        .bind(&now)
        .bind(&part_id)
        .execute(&st.pool)
        .await?;

    Ok(Json(json!({ "ok": true, "part_id": part_id, "reinstalled": true })))
}

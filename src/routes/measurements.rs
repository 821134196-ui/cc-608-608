//! 风量读数：从模拟测量仪读取（GET 触发一次仪表读数），
//! 记录入库按 (订单, 阶段) 做唯一 upsert——重复提交只覆盖更新，绝不产生重复读数。

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::error::{ApiError, ApiResult};
use crate::meter::Phase;
use crate::models::get_order;
use crate::state::AppState;

/// 读取模拟测量仪当前读数（不落库），供前端“连接测量仪”按钮展示。
pub async fn read_meter(
    State(st): State<AppState>,
    Path((id, phase)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = get_order(&st.pool, &id).await?;
    let phase = Phase::parse(&phase)
        .ok_or_else(|| ApiError::bad_request("phase 只能是 before 或 after"))?;
    let reading = st.meter.read(&id, phase);
    Ok(Json(json!({
        "order_id": id,
        "phase": phase.as_str(),
        "reading": reading,
    })))
}

#[derive(Deserialize)]
pub struct RecordReq {
    pub phase: String,
    /// 可手工指定读数；为空则从模拟测量仪实时取一次
    #[serde(default)]
    pub airflow_m3h: Option<f64>,
    #[serde(default)]
    pub note: String,
}

pub async fn record(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RecordReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    if order.status == "accepted" || order.status == "completed" {
        return Err(ApiError::conflict("订单已验收，风量记录不能再修改"));
    }
    let phase = Phase::parse(&req.phase)
        .ok_or_else(|| ApiError::bad_request("phase 只能是 before 或 after"))?;

    let (airflow, source) = match req.airflow_m3h {
        Some(v) if v > 0.0 && v.is_finite() => (v, "manual-entry"),
        Some(_) => return Err(ApiError::bad_request("风量读数必须为正数")),
        None => {
            let r = st.meter.read(&id, phase);
            (r.airflow_m3h, r.source)
        }
    };

    let now = Utc::now().to_rfc3339();
    // UPSERT：同订单同阶段只有一条；重复提交刷新读数而不是新增
    sqlx::query(
        "INSERT INTO measurements (id, order_id, phase, airflow_m3h, source, note, recorded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(order_id, phase) DO UPDATE SET
            airflow_m3h = excluded.airflow_m3h,
            source       = excluded.source,
            note         = excluded.note,
            recorded_at  = excluded.recorded_at",
    )
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .bind(&id)
    .bind(phase.as_str())
    .bind(airflow)
    .bind(source)
    .bind(&req.note)
    .bind(&now)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({
        "ok": true, "phase": phase.as_str(),
        "airflow_m3h": airflow, "source": source,
        "idempotent": "upsert by (order, phase)",
    })))
}

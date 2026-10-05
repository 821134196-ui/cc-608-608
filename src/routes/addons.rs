//! 加价 / 改方案流程。
//!
//! 铁律：
//! 1. 师傅只能“提出方案和价格”，`status=pending`；
//! 2. 只有客户明确同意（approved）后，师傅才能标记 `applied=1`（按方案施工）；
//! 3. 只有 approved AND applied 的项目才进入账单；pending / rejected 永不计费；
//! 4. 客户拒绝（rejected）后，施工仍按原范围执行，账单只有基础费；
//! 5. 同类型待确认方案只允许一个（DB 唯一索引兜底），决定不可更改（防重复提交、防反悔改价）。

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{get_order, list_addons};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ProposeReq {
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub price_cents: i64,
    #[serde(default)]
    pub extra_json: serde_json::Value,
}

const ALLOWED_CODES: &[&str] = &[
    "MODEL_MISMATCH",
    "DEEP_WHEEL",   // 风轮深度拆洗
    "OIL_PAN",      // 油网/油杯更换
    "SEAL_KIT",     // 密封件更换
    "EXTRA_SERVICE",
];

pub async fn propose_addon(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ProposeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let order = get_order(&st.pool, &id).await?;
    // 加价方案只能在施工过程中提出并谈妥；提交完工后再新增收费会绕开“完工前
    // 所有价格争议已了结”的约定，故 awaiting_acceptance/accepted/completed 一律禁止。
    // （型号不符方案例外，由系统在 pending 的到场确认时自动生成。）
    if order.status != "in_progress" {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，不能再新增收费项目；加价必须在完工提交前与客户确认",
            order.status
        )));
    }
    if !ALLOWED_CODES.contains(&req.code.as_str()) {
        return Err(ApiError::bad_request(format!(
            "未知的加价项目类型：{}",
            req.code
        )));
    }
    if req.title.trim().is_empty() {
        return Err(ApiError::bad_request("方案名称不能为空"));
    }
    if req.price_cents < 0 {
        return Err(ApiError::bad_request("加价金额不能为负"));
    }

    // 同类型若已有待确认方案：拒绝重复提出，避免用新报价顶替客户正在考虑的方案
    let existing = list_addons(&st.pool, &id).await?;
    if existing
        .iter()
        .any(|a| a.code == req.code && a.status == "pending")
    {
        return Err(ApiError::conflict(
            "该项目已存在待客户确认的方案，不能重复提交；客户决定后才能再提",
        ));
    }

    let addon_id = Uuid::new_v4().simple().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO addons
            (id, order_id, code, title, description, price_cents, extra_json, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
    )
    .bind(&addon_id)
    .bind(&id)
    .bind(&req.code)
    .bind(req.title.trim())
    .bind(&req.description)
    .bind(req.price_cents)
    .bind(req.extra_json.to_string())
    .bind(&now)
    .execute(&st.pool)
    .await?;

    Ok(Json(json!({
        "ok": true,
        "addon_id": addon_id,
        "status": "pending",
        "notice": "方案已提交，等待客户确认；客户确认前不得实施该加价项目，也不计入费用",
    })))
}

pub async fn list(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let _ = get_order(&st.pool, &id).await?;
    Ok(Json(json!({ "addons": list_addons(&st.pool, &id).await? })))
}

#[derive(Deserialize)]
pub struct DecisionReq {
    /// 仅接受 "approved"（客户同意）或 "rejected"（客户拒绝）
    pub decision: String,
    #[serde(default)]
    pub customer_note: String,
}

/// 客户对加价方案做决定。只有客户本人侧调用；决定一次生效、不可更改、不可重复提交。
pub async fn decide(
    State(st): State<AppState>,
    Path((id, addon_id)): Path<(String, String)>,
    Json(req): Json<DecisionReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let _order = get_order(&st.pool, &id).await?;
    let decision = match req.decision.as_str() {
        "approved" | "rejected" => req.decision.as_str(),
        _ => return Err(ApiError::bad_request("decision 只能是 approved 或 rejected")),
    };

    let addon = sqlx::query_as::<_, crate::models::AddonRow>(
        "SELECT * FROM addons WHERE id = ?1 AND order_id = ?2",
    )
    .bind(&addon_id)
    .bind(&id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("加价方案不存在"))?;

    if addon.status != "pending" {
        // 幂等冲突：已经做过决定 → 明确告知，且保持原结果不变
        return Err(ApiError::conflict(format!(
            "该方案已{}，不能重复提交决定",
            if addon.status == "approved" { "获客户同意" } else { "被客户拒绝" }
        )));
    }

    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE addons SET status=?1, decided_at=?2,
            extra_json=json_set(COALESCE(extra_json,'{}'), '$.customer_note', ?3)
         WHERE id=?4 AND status='pending'",
    )
    .bind(decision)
    .bind(&now)
    .bind(&req.customer_note)
    .bind(&addon_id)
    .execute(&st.pool)
    .await?;

    // 并发兜底：WHERE status='pending' 保证只有一个决定能落库
    if res.rows_affected() != 1 {
        return Err(ApiError::conflict("该方案已被处理，重复提交无效"));
    }

    if decision == "approved" {
        Ok(Json(json!({
            "ok": true, "status": "approved",
            "notice": "客户已同意，师傅可按此方案施工；施工完成后该项目才计入费用",
        })))
    } else {
        Ok(Json(json!({
            "ok": true, "status": "rejected",
            "notice": "客户已拒绝该加价项目，继续按原范围执行，不收取该费用",
        })))
    }
}

/// 师傅确认已按客户同意的方案完成施工。
/// 未获同意（pending/rejected）的项目禁止施工标记——未确认加价不会生效。
pub async fn mark_applied(
    State(st): State<AppState>,
    Path((id, addon_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let _order = get_order(&st.pool, &id).await?;
    let addon = sqlx::query_as::<_, crate::models::AddonRow>(
        "SELECT * FROM addons WHERE id = ?1 AND order_id = ?2",
    )
    .bind(&addon_id)
    .bind(&id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("加价方案不存在"))?;

    if addon.applied == 1 {
        return Err(ApiError::conflict("该项目已标记施工，请勿重复提交"));
    }
    match addon.status.as_str() {
        "pending" => {
            return Err(ApiError::conflict(
                "客户尚未确认该方案，不能施工、不能收费",
            ));
        }
        "rejected" => {
            return Err(ApiError::conflict(
                "客户已拒绝该方案，必须按原范围执行，不能施工该加价项目",
            ));
        }
        "approved" => {}
        other => return Err(ApiError::Internal(anyhow::anyhow!("未知方案状态 {other}"))),
    }

    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE addons SET applied=1, applied_at=?1 WHERE id=?2 AND status='approved' AND applied=0",
    )
    .bind(&now)
    .bind(&addon_id)
    .execute(&st.pool)
    .await?;
    if res.rows_affected() != 1 {
        return Err(ApiError::conflict("施工登记失败或已登记，请勿重复提交"));
    }

    Ok(Json(json!({ "ok": true, "applied": true })))
}
